//! JDK methods Scala Native's javalib leaves out or stubs, which the VM can provide:
//! `Thread.getStackTrace` of the current thread and `Runtime`'s memory figures.

use super::{NativeFn, unwind};
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

/// `thread.getStackTrace()`: for the current thread, `Thread.getStackTrace` itself and then its
/// callers' frames, the way the JDK lists them; javalib's empty array for other threads.
fn thread_stack_trace(vm: &mut Vm, args: &[u64]) -> Flow {
    if args[0] != heap::read::<u64>(vm.thread) {
        let thread = vm.world.top("java.lang.Thread");
        let own = vm.world.member(thread, STACK_TRACE);
        let original = vm.original(own)?;
        return vm.call(original, args);
    }
    let ips = unwind::frame_ips(vm, unwind::MAX_FRAMES - 1);
    let array = vm.link_top("scala.scalanative.runtime.ObjectArray")?;
    let out = vm.alloc_array(array, ips.len() + 1);
    let here = unwind::new_element(vm, "java.lang.Thread", "getStackTrace", None, -1)?;
    heap::write(out + ARRAY_HEADER as usize, here);
    for (i, &ip) in ips.iter().enumerate() {
        let element = unwind::element_of(vm, ip)?;
        heap::write(out + ARRAY_HEADER as usize + (i + 1) * 8, element);
    }
    Ok(out as u64)
}
