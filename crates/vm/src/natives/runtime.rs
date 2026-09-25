//! Rust implementations of C runtime externs and of runtime methods the VM replaces.

use super::NativeFn;
use crate::Vm;
use crate::interp::Flow;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name {
        "scalanative_currentThread" => |vm, _| Ok(vm.thread.java),
        "scalanative_currentNativeThread" => |vm, _| Ok(vm.thread.native),
        "scalanative_assignCurrentThread" => assign_thread,
        "scalanative_setupCurrentThreadInfo" | "scalanative_Throwable_sizeOfExceptionWrapper" => |_, _| Ok(0),
        _ => return super::llvm::intrinsic(name),
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
        _ => return None,
    };
    Some(f)
}

fn empty_stack_trace(vm: &mut Vm, _: &[u64]) -> Flow {
    let cls = vm.link_top("scala.scalanative.runtime.LongArray")?;
    Ok(vm.alloc_array(cls, 0) as u64)
}
