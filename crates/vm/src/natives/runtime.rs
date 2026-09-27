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
        | "scalanative_Throwable_sizeOfExceptionWrapper"
        | "scalanative_resource_get_embedded_size"
        | "scalanative_resource_get_content_length"
        | "scalanative_resource_get_content_ptr"
        | "scalanative_resource_get_path_length"
        | "scalanative_resource_get_path_ptr" => |_, _| Ok(0),
        _ => {
            return super::llvm::intrinsic(name)
                .or_else(|| super::gc::extern_fn(name))
                .or_else(|| super::unwind::extern_fn(name))
                .or_else(|| super::threads::extern_fn(name))
                .or_else(|| super::junit::extern_fn(name));
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
        ("java.util.HashSet", s) if s.starts_with("D22containsAll$$anonfun$1") => hash_set_contains,
        _ => {
            return super::services::method(owner, sig)
                .or_else(|| super::unwind::method(owner, sig))
                .or_else(|| super::threads::method(owner, sig));
        }
    };
    Some(f)
}

/// `HashSet.containsAll`'s `forall(contains)` lambda, which the published javalib compiled with a
/// `Nothing` parameter into `throw o`: `this.contains(o)`.
fn hash_set_contains(vm: &mut Vm, args: &[u64]) -> Flow {
    let sig = nir::StrId(vm.world.pool.strings.intern("D8containsL16java.lang.ObjectzEO".into()));
    let f = vm.virtual_method(args[0] as usize, sig)?;
    vm.call(f, &args[..2])
}

/// `java.lang.Class.is(left, right)`: whether `left` is `right` or a subtype of it.
fn class_is(vm: &mut Vm, args: &[u64]) -> Flow {
    let (left, right) = (vm.class_of_rtti(args[1] as usize), vm.class_of_rtti(args[2] as usize));
    Ok(vm.is_subclass(left, right) as u64)
}
