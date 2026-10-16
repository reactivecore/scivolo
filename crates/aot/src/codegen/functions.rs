//! The reached functions: each translated to Cranelift IR in turn, then a batch of them compiled
//! on all cores and defined in the object.

use super::{Decls, body, replaced, stubs};
use crate::R;
use crate::reach::Reach;
use crate::timings::Timings;
use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Module, ModuleReloc};
use cranelift_object::ObjectModule;
use nir::{DefnKind, GlobalId};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Functions compiled per thread at least; smaller batches use fewer threads.
const PER_THREAD: usize = 16;
/// Translated functions held at most before they are compiled.
pub const BATCH: usize = 1024;

/// A function translated to Cranelift IR, not compiled yet.
pub struct Translated {
    id: FuncId,
    g: GlobalId,
    ctx: Context,
}

/// Function `g` translated to Cranelift IR.
pub fn translate(
    module: &mut ObjectModule,
    reach: &mut Reach,
    decls: &mut Decls,
    fb: &mut FunctionBuilderContext,
    g: GlobalId,
) -> R<Translated> {
    let defn = reach.world.defn(g).map_err(|e| e.to_string())?;
    let DefnKind::Define { ty, body } = &defn.kind else { unreachable!("reached a non-function") };
    let mut ctx = Context::new();
    ctx.func.signature = decls.signature(&reach.world.pool, *ty)?;
    let id = decls.func(module, reach, g)?;
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
            let name = reach.world.name(g);
            stubs::missing(module, decls, (&mut ctx, fb), &format!("{name}: {e}"))?;
        }
        Err(e) => return Err(format!("{}: {e}", reach.world.name(g))),
    }
    Ok(Translated { id, g, ctx })
}

/// Compiles the functions of `batch` on all cores, then defines them in `module` and records
/// them in the code-address table.
pub fn compile(
    module: &mut ObjectModule,
    reach: &Reach,
    decls: &mut Decls,
    batch: &mut Vec<Translated>,
    timings: &mut Timings,
) -> R<()> {
    let clock = Instant::now();
    let isa = module.isa();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = cores.min(batch.len().div_ceil(PER_THREAD));
    let (todo, next) = (batch.iter_mut().map(Mutex::new).collect::<Vec<_>>(), AtomicUsize::new(0));
    let work = || -> Option<(GlobalId, String)> {
        while let Some(cell) = todo.get(next.fetch_add(1, Ordering::Relaxed)) {
            let mut t = cell.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = t.ctx.compile(isa, &mut ControlPlane::default()) {
                let message = format!("{:?}", e.inner);
                return Some((t.g, message));
            }
        }
        None
    };
    let failed = match threads {
        0 | 1 => work(),
        _ => std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads).map(|_| s.spawn(work)).collect();
            let results =
                workers.into_iter().map(|w| w.join().unwrap_or_else(|p| std::panic::resume_unwind(p)));
            results.collect::<Vec<_>>().into_iter().flatten().next()
        }),
    };
    drop(todo);
    if let Some((g, e)) = failed {
        return Err(format!("{}: {e}", reach.world.name(g)));
    }
    for t in batch.drain(..) {
        let code = t.ctx.compiled_code().ok_or("a function was not compiled")?;
        let relocs: Vec<ModuleReloc> =
            code.buffer.relocs().iter().map(|r| ModuleReloc::from_mach_reloc(r, &t.ctx.func, t.id)).collect();
        module
            .define_function_bytes(t.id, code.buffer.alignment as u64, code.buffer.data(), &relocs)
            .map_err(|e| format!("{}: {e:?}", reach.world.name(t.g)))?;
        decls.frames.record(t.id, t.g, &t.ctx);
    }
    timings.part("reach+codegen", "cranelift", clock.elapsed());
    Ok(())
}
