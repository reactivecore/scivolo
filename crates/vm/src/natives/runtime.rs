//! Rust implementations of C runtime externs and of runtime methods the VM replaces.

use super::NativeFn;
use crate::Vm;
use crate::interp::Flow;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name {
        "scalanative_currentThread" => |vm, _| Ok(vm.thread.java),
        "scalanative_currentNativeThread" => |vm, _| Ok(vm.thread.native),
        "scalanative_assignCurrentThread" => assign_thread,
        "scalanative_setupCurrentThreadInfo"
        | "scalanative_Throwable_sizeOfExceptionWrapper"
        | "scalanative_resource_get_embedded_size"
        | "scalanative_resource_get_content_length"
        | "scalanative_resource_get_content_ptr"
        | "scalanative_resource_get_path_length"
        | "scalanative_resource_get_path_ptr" => |_, _| Ok(0),
        _ => {
            return super::llvm::intrinsic(name)
                .or_else(|| super::gc::extern_fn(name))
                .or_else(|| super::junit::extern_fn(name));
        }
    };
    Some(f)
}

fn assign_thread(vm: &mut Vm, args: &[u64]) -> Flow {
    vm.thread.java = args[0];
    vm.thread.native = args[1];
    Ok(0)
}

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        ("scala.scalanative.runtime.StackTrace$", "D20currentRawStackTraceLAj_EO") => empty_stack_trace,
        ("java.lang.Class", "D2isL15java.lang.ClassL15java.lang.ClasszEPT15java.lang.Class") => class_is,
        _ => return super::services::method(owner, sig),
    };
    Some(f)
}

/// `java.lang.Class.is(left, right)`: whether `left` is `right` or a subtype of it.
fn class_is(vm: &mut Vm, args: &[u64]) -> Flow {
    let (left, right) = (vm.class_of_rtti(args[1] as usize), vm.class_of_rtti(args[2] as usize));
    Ok(vm.is_subclass(left, right) as u64)
}

fn empty_stack_trace(vm: &mut Vm, _: &[u64]) -> Flow {
    let cls = vm.link_top("scala.scalanative.runtime.LongArray")?;
    Ok(vm.alloc_array(cls, 0) as u64)
}
