//! NIR → Cranelift IR → one object file: the functions reachable from the entry, the module
//! load functions and the class metadata.

mod arrays;
mod blocks;
mod body;
mod checks;
mod consts;
mod decls;
mod intrinsics;
mod linktime;
mod memory;
mod modules;
mod objects;
mod ops;
mod rtti_layout;
mod start;
mod strings;
mod unwind;
mod values;
mod variadic;

pub use decls::Decls;
pub use rtti_layout::{CLASS, RttiLayout};

use crate::R;
use crate::reach::Reach;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{Context, isa};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
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
    let meta = reach.link_top(CLASS)?;
    reach.allocate(meta);
    crate::tables::link_runtime(&mut reach)?;
    match root {
        Root::Entry(entry) => {
            let target = decls.func(&mut module, &mut reach, entry)?;
            entry_wrapper(&mut module, &decls, target)?;
        }
        Root::Main(top) => start::reach_main(&mut module, &mut reach, &mut decls, top)?,
    }
    let mut ctx = Context::new();
    let mut fb = FunctionBuilderContext::new();
    loop {
        while let Some(g) = reach.queue.pop() {
            define(&mut module, &mut reach, &mut decls, (&mut ctx, &mut fb), g)?;
        }
        for g in reach.dispatch_targets() {
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
    let mut f = body::FnBuilder::new(FunctionBuilder::new(&mut ctx.func, fb), module, reach, decls);
    f.body(body).map_err(|e| format!("{name}: {e}"))?;
    f.finish();
    module.define_function(id, ctx).map_err(|e| format!("{name}: {e:?}"))?;
    module.clear_context(ctx);
    Ok(())
}

/// `scalai_entry(n: i32) -> i32`, which calls `target`.
fn entry_wrapper(module: &mut ObjectModule, decls: &Decls, target: FuncId) -> R<()> {
    let mut sig = Signature::new(decls.call_conv);
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    let id = module.declare_function("scalai_entry", Linkage::Export, &sig).map_err(|e| e.to_string())?;
    let mut ctx = Context::new();
    ctx.func.signature = sig;
    let mut fb = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
    let block = b.create_block();
    b.append_block_params_for_function_params(block);
    b.switch_to_block(block);
    let callee = module.declare_func_in_func(target, b.func);
    let n = b.block_params(block)[0];
    let call = b.ins().call(callee, &[n]);
    let r = b.inst_results(call)[0];
    b.ins().return_(&[r]);
    b.seal_all_blocks();
    b.finalize(module.target_config());
    module.define_function(id, &mut ctx).map_err(|e| format!("{e:?}"))
}
