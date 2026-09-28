//! Rust implementations of C runtime externs and of runtime methods the VM replaces.

use super::NativeFn;
use crate::interp::Flow;
use crate::{Vm, heap};

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name {
        "scalanative_currentThread" => |vm, _| Ok(heap::read(vm.thread)),
        "scalanative_currentNativeThread" => |vm, _| Ok(heap::read(vm.thread + 8)),
        "scalanative_assignCurrentThread" => assign_thread,
        "scalanative_setupCurrentThreadInfo"
        | "scalanative_StackOverflowGuards_setup"
        | "scalanative_StackOverflowGuards_reset"
        | "scalanative_StackOverflowGuards_close"
        | "scalanative_StackOverflowGuards_size"
        | "scalanative_Throwable_sizeOfExceptionWrapper" => |_, _| Ok(0),
        _ => {
            return super::llvm::intrinsic(name)
                .or_else(|| super::resources::extern_fn(name))
                .or_else(|| super::gc::extern_fn(name))
                .or_else(|| super::unwind::extern_fn(name))
                .or_else(|| super::threads::extern_fn(name))
                .or_else(|| super::junit::extern_fn(name))
                .or_else(|| super::ffi::extern_fn(name));
        }
    };
    Some(f)
}

fn assign_thread(vm: &mut Vm, args: &[u64]) -> Flow {
    heap::write(vm.thread, args[0]);
    heap::write(vm.thread + 8, args[1]);
    vm.set_native_thread(args[1] as usize);
    Ok(0)
}

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        ("java.lang.Class", "D2isL15java.lang.ClassL15java.lang.ClasszEPT15java.lang.Class") => class_is,
        ("java.lang.Class$", "D7forNameL16java.lang.StringL15java.lang.ClassEO") => class_for_name,
        ("scala.scalanative.runtime.Intrinsics$", "D12multiplyHighjjjEO") => {
            |_, args| Ok((((args[1] as i64 as i128) * (args[2] as i64 as i128)) >> 64) as u64)
        }
        ("scala.scalanative.runtime.Intrinsics$", "D20unsignedMultiplyHighjjjEO") => {
            |_, args| Ok((((args[1] as u128) * (args[2] as u128)) >> 64) as u64)
        }
        ("java.util.HashSet", s) if s.starts_with("D22containsAll$$anonfun$1") => hash_set_contains,
        ("java.util.concurrent.ForkJoinPool$", HELP_ASYNC_BLOCKER) => help_async_blocker,
        _ => {
            return super::services::method(owner, sig)
                .or_else(|| super::unwind::method(owner, sig))
                .or_else(|| super::threads::method(owner, sig));
        }
    };
    Some(f)
}

const HELP_ASYNC_BLOCKER: &str = "D16helpAsyncBlockerL29java.util.concurrent.ExecutorL48java.util.concurrent.ForkJoinPool$ManagedBlockeruEO";

/// `ForkJoinPool.helpAsyncBlocker(e, blocker)`, whose javalib version matches the current thread
/// against `ForkJoinWorkerThread` only: nothing on other threads, the javalib method on workers.
fn help_async_blocker(vm: &mut Vm, args: &[u64]) -> Flow {
    let thread = heap::read::<u64>(vm.thread) as usize;
    let worker = vm.link_top("java.util.concurrent.ForkJoinWorkerThread")?;
    if thread == 0 || !vm.is_subclass(vm.class_of(thread), worker) {
        return Ok(vm.unit()? as u64);
    }
    let owner = vm.world.top("java.util.concurrent.ForkJoinPool$");
    let method = vm.world.member(owner, HELP_ASYNC_BLOCKER);
    let original = vm.original(method)?;
    vm.call(original, args)
}

/// `HashSet.containsAll`'s `forall(contains)` lambda, which the published javalib compiled with a
/// `Nothing` parameter into `throw o`: `this.contains(o)`.
fn hash_set_contains(vm: &mut Vm, args: &[u64]) -> Flow {
    let sig = nir::StrId(vm.world.pool.strings.intern("D8containsL16java.lang.ObjectzEO".into()));
    let f = vm.virtual_method(args[0] as usize, sig)?;
    vm.call(f, &args[..2])
}

/// `Class.forName(name)`: the class, module or trait `name` on the classpath, linked and
/// initialized, else `ClassNotFoundException`.
fn class_for_name(vm: &mut Vm, args: &[u64]) -> Flow {
    if args[1] == 0 {
        return vm.null_pointer();
    }
    let name = vm.read_string(args[1] as usize)?;
    let well_formed = !name.contains(['/', '[']) && name.split('.').all(|part| !part.is_empty());
    if !well_formed {
        return vm.throw_new_with("java.lang.ClassNotFoundException", &name);
    }
    let g = vm.world.top(&name);
    let is_class = vm.world.defn(g).is_ok_and(|d| {
        matches!(
            d.kind,
            nir::DefnKind::Class { .. } | nir::DefnKind::Module { .. } | nir::DefnKind::Trait { .. }
        )
    });
    if !is_class {
        return vm.throw_new_with("java.lang.ClassNotFoundException", &name);
    }
    let cls = vm.link(g)?;
    Ok(vm.class(cls).rtti as u64)
}

/// `java.lang.Class.is(left, right)`: whether `left` is `right` or a subtype of it.
fn class_is(vm: &mut Vm, args: &[u64]) -> Flow {
    let (left, right) = (vm.class_of_rtti(args[1] as usize), vm.class_of_rtti(args[2] as usize));
    Ok(vm.is_subclass(left, right) as u64)
}
