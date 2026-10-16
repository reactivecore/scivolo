//! `scalai_aot_main(argc, argv)`, which runs a program as Scala Native's generated `main` does:
//! static initializers, `Runtime.init`, the main class's `main`, then `onShutdown`; an uncaught
//! exception goes to the current thread's uncaught exception handler and makes it return 1.

use super::{Decls, RUNTIME};
use crate::R;
use crate::reach::Reach;
use cranelift_codegen::Context;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, Signature, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::{DefnKind, GlobalId};

const INIT: &str = "D4initiR_LAL16java.lang.String_EO";
const SHUTDOWN: &str = "D10onShutdownuEO";
const MAIN: &str = "D4mainLAL16java.lang.String_uEo";
const THREAD: &str = "java.lang.Thread";

/// The member of `top` whose signature starts with `prefix`.
fn member(reach: &mut Reach, top: &str, prefix: &str) -> R<GlobalId> {
    let g = reach.world.top(top);
    let members = reach.world.members(g).map_err(|e| e.to_string())?.to_vec();
    let pool = &reach.world.pool;
    let found = members.into_iter().find(|&m| pool.member_sig(m).is_some_and(|s| s.starts_with(prefix)));
    found.ok_or_else(|| format!("no {top}::{prefix}"))
}

/// `Thread.currentThread`, `Thread.getUncaughtExceptionHandler` and
/// `Runtime.executeUncaughtExceptionHandler`, which report an uncaught exception.
fn handlers(reach: &mut Reach) -> R<[GlobalId; 3]> {
    Ok([
        member(reach, THREAD, "D13currentThread")?,
        member(reach, THREAD, "D27getUncaughtExceptionHandler")?,
        member(reach, RUNTIME, "D31executeUncaughtExceptionHandler")?,
    ])
}

/// The static initializers of the linked classes, reached for compilation; whether any was new.
pub fn reach_initializers(module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls) -> R<bool> {
    let mut new = false;
    for i in 0..reach.classes.all.len() {
        let clinit = reach.world.member(reach.classes.all[i].name, "IE");
        let defined = reach.world.defn(clinit).is_ok_and(|d| matches!(d.kind, DefnKind::Define { .. }));
        if defined && !decls.has_func(clinit) {
            decls.func(module, reach, clinit)?;
            new = true;
        }
    }
    Ok(new)
}

/// Reaches what the generated main calls: the runtime's init and shutdown, and `main` of `top`.
pub fn reach_main(module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls, top: GlobalId) -> R<()> {
    let runtime = reach.link_top(RUNTIME)?;
    decls.module_load(module, reach, runtime)?;
    let name = reach.classes.get(runtime).name;
    for sig in [INIT, SHUTDOWN] {
        let g = reach.world.member(name, sig);
        decls.func(module, reach, g)?;
    }
    let main = reach.world.member(top, MAIN);
    decls.func(module, reach, main)?;
    reach.link_top(THREAD)?;
    for g in handlers(reach)? {
        decls.func(module, reach, g)?;
    }
    Ok(())
}

pub fn define_main(module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls, top: GlobalId) -> R<()> {
    let ptr = decls.ptr;
    let runtime = reach.link_top(RUNTIME)?;
    let name = reach.classes.get(runtime).name;
    let (init, shutdown) = (reach.world.member(name, INIT), reach.world.member(name, SHUTDOWN));
    let main = reach.world.member(top, MAIN);
    let mut clinits: Vec<FuncId> = Vec::new();
    for i in 0..reach.classes.all.len() {
        let clinit = reach.world.member(reach.classes.all[i].name, "IE");
        if decls.has_func(clinit) {
            clinits.push(decls.func(module, reach, clinit)?);
        }
    }
    let [load, init, shutdown, main] = [
        decls.module_load(module, reach, runtime)?,
        decls.func(module, reach, init)?,
        decls.func(module, reach, shutdown)?,
        decls.func(module, reach, main)?,
    ];
    let [current, get_handler, execute] = handlers(reach)?;
    let [current, get_handler, execute] = [
        decls.func(module, reach, current)?,
        decls.func(module, reach, get_handler)?,
        decls.func(module, reach, execute)?,
    ];
    let mut sig = Signature::new(decls.call_conv);
    sig.params.extend([AbiParam::new(types::I32), AbiParam::new(ptr)]);
    sig.returns.push(AbiParam::new(types::I32));
    let id = module.declare_function("scalai_aot_main", Linkage::Export, &sig).map_err(|e| e.to_string())?;
    let mut ctx = Context::new();
    ctx.func.signature = sig;
    let mut fb = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
    let block = b.create_block();
    b.append_block_params_for_function_params(block);
    b.switch_to_block(block);
    let (argc, argv) = (b.block_params(block)[0], b.block_params(block)[1]);
    let uncaught = b.create_block();
    let slot = module.declare_data_in_func(decls.exception, b.func);
    let mut call = |b: &mut FunctionBuilder, f: FuncId, args: &[cranelift_codegen::ir::Value]| {
        let fref = module.declare_func_in_func(f, b.func);
        let inst = b.ins().call(fref, args);
        let result = b.inst_results(inst).first().copied();
        let addr = b.ins().symbol_value(ptr, slot);
        let exc = b.ins().load(ptr, MemFlagsData::new(), addr, 0);
        let cont = b.create_block();
        b.ins().brif(exc, uncaught, &[], cont, &[]);
        b.switch_to_block(cont);
        result
    };
    for f in clinits {
        call(&mut b, f, &[]);
    }
    let rt = call(&mut b, load, &[]).ok_or("module load without result")?;
    let args = call(&mut b, init, &[rt, argc, argv]).ok_or("Runtime.init without result")?;
    call(&mut b, main, &[args]);
    call(&mut b, shutdown, &[rt]);
    let zero = b.ins().iconst(types::I32, 0);
    b.ins().return_(&[zero]);
    b.switch_to_block(uncaught);
    let addr = b.ins().symbol_value(ptr, slot);
    let exc = b.ins().load(ptr, MemFlagsData::new(), addr, 0);
    let null = b.ins().iconst(ptr, 0);
    b.ins().store(MemFlagsData::new(), null, addr, 0);
    let mut plain = |b: &mut FunctionBuilder, f: FuncId, args: &[cranelift_codegen::ir::Value]| {
        let fref = module.declare_func_in_func(f, b.func);
        let inst = b.ins().call(fref, args);
        b.inst_results(inst).first().copied()
    };
    let rt = plain(&mut b, load, &[]).ok_or("module load without result")?;
    let thread = plain(&mut b, current, &[]).ok_or("currentThread without result")?;
    let handler =
        plain(&mut b, get_handler, &[thread]).ok_or("getUncaughtExceptionHandler without result")?;
    plain(&mut b, execute, &[rt, handler, thread, exc]);
    let one = b.ins().iconst(types::I32, 1);
    b.ins().return_(&[one]);
    b.seal_all_blocks();
    b.finalize(module.target_config());
    module.define_function(id, &mut ctx).map_err(|e| format!("{e:?}"))
}
