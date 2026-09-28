//! JDK methods Scala Native's javalib leaves out or stubs, which the VM can provide:
//! `Thread.getStackTrace` of the current thread and `Runtime`'s memory figures.

use super::NativeFn;
use crate::interp::Flow;
use crate::layout::ARRAY_HEADER;
use crate::{Vm, heap};

const STACK_TRACE: &str = "D13getStackTraceLAL27java.lang.StackTraceElement_EO";

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        ("java.lang.Thread", STACK_TRACE) => thread_stack_trace,
        ("java.lang.Runtime$", "D11totalMemoryjEO") => |vm, _| Ok(vm.heap.sizes().0 as u64),
        ("java.lang.Runtime$", "D9maxMemoryjEO") => |vm, _| Ok(vm.heap.sizes().1 as u64),
        ("java.lang.Runtime$", "D10freeMemoryjEO") => |_, _| Ok(0),
        _ => return None,
    };
    Some(f)
}

/// `thread.getStackTrace()`: for the current thread, its frames from this call outwards, the
/// way the JDK lists them; javalib's empty array for other threads.
fn thread_stack_trace(vm: &mut Vm, args: &[u64]) -> Flow {
    let thread = vm.world.top("java.lang.Thread");
    let own = vm.world.member(thread, STACK_TRACE);
    if args[0] != heap::read::<u64>(vm.thread) {
        let original = vm.original(own)?;
        return vm.call(original, args);
    }
    let throwable = vm.link_top("java.lang.Throwable")?;
    let exc = vm.alloc_object(throwable) as u64;
    let name = vm.class(throwable).name;
    let init = vm.world.member(name, "RE");
    vm.call_global(init, &[exc])?;
    let trace = vm.world.member(name, STACK_TRACE);
    let frames = vm.call_global(trace, &[exc])? as usize;
    let here = element(vm, "java.lang.Thread", "getStackTrace")?;
    let len = vm.array_length(frames as u64)? as usize;
    let out = vm.alloc_array(vm.class_of(frames), len + 1);
    heap::write(out + ARRAY_HEADER as usize, here);
    heap::copy(frames + ARRAY_HEADER as usize, out + ARRAY_HEADER as usize + 8, len * 8);
    Ok(out as u64)
}

/// A `StackTraceElement` for method `method` of class `class`, without a source position.
fn element(vm: &mut Vm, class: &str, method: &str) -> Flow {
    let cls = vm.link_top("java.lang.StackTraceElement")?;
    let obj = vm.alloc_object(cls) as u64;
    let (class, method) = (vm.new_string(class)? as u64, vm.new_string(method)? as u64);
    let init =
        vm.world.member(vm.class(cls).name, "RL16java.lang.StringL16java.lang.StringL16java.lang.StringiE");
    vm.call_global(init, &[obj, class, method, 0, (-1i32) as u32 as u64])?;
    Ok(obj)
}
