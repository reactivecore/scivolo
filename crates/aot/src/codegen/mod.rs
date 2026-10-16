//! NIR → Cranelift IR → one object file: the functions reachable from the entry, the module
//! load functions and the class metadata.

mod arrays;
mod blocks;
mod body;
mod branches;
mod checks;
mod consts;
mod decls;
mod elements;
mod frames;
mod intrinsics;
mod linktime;
mod memory;
mod modules;
mod objects;
mod ops;
mod overflow;
mod replaced;
mod rtti_layout;
mod stack_check;
mod start;
mod strings;
mod stubs;
mod type_tests;
mod unwind;
mod values;
mod variadic;

pub use decls::Decls;
pub use rtti_layout::{CLASS, RttiLayout};

/// The module of Scala Native's runtime functions the generated code calls.
pub const RUNTIME: &str = "scala.scalanative.runtime.package$";

use crate::R;
use crate::reach::Reach;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{Context, isa};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use nir::{DefnKind, GlobalId};
use vm::World;

fn object_module() -> R<ObjectModule> {
    let mut flags = settings::builder();
    let set = |f: &mut settings::Builder, k, v| f.set(k, v).map_err(|e| e.to_string());
    set(&mut flags, "opt_level", "speed")?;
    set(&mut flags, "is_pic", "true")?;
    set(&mut flags, "preserve_frame_pointers", "true")?;
    let isa = isa::lookup(target_lexicon::Triple::host())
        .map_err(|e| e.to_string())?
        .finish(settings::Flags::new(flags))
        .map_err(|e| e.to_string())?;
    let builder =
        ObjectBuilder::new(isa, "scalai-aot", default_libcall_names()).map_err(|e| e.to_string())?;
    Ok(ObjectModule::new(builder))
}

/// What the executable runs.
pub enum Root {
    /// An `(Int) => Int` method, called by the entry stub, for tests.
    Entry(GlobalId),
    /// The main class, run by `scalai_aot_main`.
    Main(GlobalId),
}

/// The object file and the C source of the variadic call shims it calls.
pub fn compile(world: &mut World, root: Root) -> R<(Vec<u8>, String)> {
    let mut module = object_module()?;
    let mut reach = Reach::new(world);
    let rtti = RttiLayout::new(&mut reach)?;
    let mut decls = Decls::new(&mut module, rtti)?;
    for class in [CLASS, "java.lang.String", "scala.scalanative.runtime.CharArray"] {
        let cls = reach.link_top(class)?;
        reach.allocate(cls);
    }
    crate::tables::link_runtime(&mut reach)?;
    match root {
        Root::Entry(entry) => {
            decls.stack_checks = false;
            let target = decls.func(&mut module, &mut reach, entry)?;
            stubs::entry_wrapper(&mut module, &decls, target)?;
        }
        Root::Main(top) => start::reach_main(&mut module, &mut reach, &mut decls, top)?,
    }
    let mut ctx = Context::new();
    let mut fb = FunctionBuilderContext::new();
    loop {
        while let Some(g) = reach.queue.pop() {
            define(&mut module, &mut reach, &mut decls, (&mut ctx, &mut fb), g)?;
        }
        for g in reach.dispatch_targets()? {
            decls.func(&mut module, &mut reach, g)?;
        }
        if matches!(root, Root::Main(_)) {
            start::reach_initializers(&mut module, &mut reach, &mut decls)?;
        }
        if reach.queue.is_empty() {
            break;
        }
    }
    if let Root::Main(top) = root {
        start::define_main(&mut module, &mut reach, &mut decls, top)?;
    }
    modules::define_loads(&mut module, &mut reach, &mut decls)?;
    crate::tables::define(&mut module, &mut reach, &mut decls)?;
    std::mem::take(&mut decls.frames).define(&mut module, &mut reach, &mut decls)?;    decls.define_roots(&mut module)?;
    let shims = format!("#include <stdint.h>\n{}", decls.shims.concat());
    Ok((module.finish().emit().map_err(|e| e.to_string())?, shims))
}

fn define(
    module: &mut ObjectModule,
    reach: &mut Reach,
    decls: &mut Decls,
    (ctx, fb): (&mut Context, &mut FunctionBuilderContext),
    g: GlobalId,
) -> R<()> {
    let defn = reach.world.defn(g).map_err(|e| e.to_string())?;
    let DefnKind::Define { ty, body } = &defn.kind else { unreachable!("reached a non-function") };
    ctx.func.signature = decls.signature(&reach.world.pool, *ty)?;
    let id = decls.func(module, reach, g)?;
    let name = reach.world.name(g);
    let pool = &reach.world.pool;
    let owner = reach.world.owner(g);
    let (ret, this) = match pool.ty(*ty) {
        nir::Type::Function(params, ret) => {
            let this = params
                .first()
                .is_some_and(|&p| matches!(pool.ty(p), nir::Type::Ref { name, .. } if *name == owner));
            (Some(*ret), this)
        }
        _ => (None, false),
    };
    let mut f = body::FnBuilder::new(FunctionBuilder::new(&mut ctx.func, fb), module, reach, decls);
    f.ret_ty = ret;
    f.has_this = this;
    let built = match replaced::replaced(&f.reach.world.pool, g) {
        Some(r) => f.replaced_body(r),
        None => f.body(body),
    };
    match built {
        Ok(()) => f.finish(),
        // Temporary until #115 (precise reachability): code reached only through the
        // over-approximated dispatch may name classes the classpath lacks; it stops the program.
        Err(e) if e.starts_with("no definition of") => {
            drop(f);
            stubs::missing(module, decls, (ctx, fb), &format!("{name}: {e}"))?;
        }
        Err(e) => return Err(format!("{name}: {e}")),
    }
    module.define_function(id, ctx).map_err(|e| format!("{name}: {e:?}"))?;
    decls.frames.record(id, g, ctx);
    module.clear_context(ctx);
    Ok(())
}
