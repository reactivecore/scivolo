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
mod functions;
mod intrinsics;
mod linktime;
mod memory;
mod modules;
mod objects;
mod ops;
mod overflow;
mod replaced;
mod rtti_layout;
mod services;
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
use crate::timings::Timings;
use cranelift_codegen::isa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::default_libcall_names;
use cranelift_object::{ObjectBuilder, ObjectModule};
use nir::GlobalId;
use std::time::Instant;
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
pub fn compile(world: &mut World, root: Root, timings: &mut Timings) -> R<(Vec<u8>, String)> {
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
    // Temporary until #115 (library cache): library functions are compiled again for every
    // program.
    let mut fb = FunctionBuilderContext::new();
    let mut batch = Vec::new();
    loop {
        while let Some(g) = reach.queue.pop() {
            batch.push(functions::translate(&mut module, &mut reach, &mut decls, &mut fb, g)?);
            if batch.len() >= functions::BATCH {
                functions::compile(&mut module, &reach, &mut decls, &mut batch, timings)?;
            }
        }
        functions::compile(&mut module, &reach, &mut decls, &mut batch, timings)?;
        let clock = Instant::now();
        for g in reach.dispatch_targets()? {
            decls.func(&mut module, &mut reach, g)?;
        }
        timings.part("reach+codegen", "dispatch", clock.elapsed());
        if matches!(root, Root::Main(_)) {
            let clock = Instant::now();
            start::reach_initializers(&mut module, &mut reach, &mut decls)?;
            timings.part("reach+codegen", "initializers", clock.elapsed());
        }
        if reach.queue.is_empty() && !reach.undispatched() {
            break;
        }
    }
    if let Root::Main(top) = root {
        start::define_main(&mut module, &mut reach, &mut decls, top)?;
    }
    timings.lap("reach+codegen");
    modules::define_loads(&mut module, &mut reach, &mut decls)?;
    crate::tables::define(&mut module, &mut reach, &mut decls)?;
    std::mem::take(&mut decls.frames).define(&mut module, &mut reach, &mut decls)?;
    crate::resources::define(&mut module, reach.world)?;
    decls.define_roots(&mut module)?;
    let shims = format!("#include <stdint.h>\n{}", decls.shims.concat());
    timings.lap("tables");
    let object = module.finish().emit().map_err(|e| e.to_string())?;
    timings.lap("emit");
    Ok((object, shims))
}
