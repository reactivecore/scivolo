//! The `scalanative_GC_*` interface on top of the VM heap, which never collects.

use super::NativeFn;
use crate::Vm;
use crate::heap;
use crate::interp::Flow;
use crate::layout::{ARRAY_LENGTH, ARRAY_STRIDE};

/// Heap sizes reported to programs; the heap is only limited by memory.
const INIT_HEAP: u64 = 1 << 20;
const MAX_HEAP: u64 = 16 << 30;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name.strip_prefix("scalanative_GC_")? {
        "alloc" | "alloc_small" | "alloc_large" => alloc,
        "alloc_array" => alloc_array,
        "get_used_heapsize" => |vm, _| Ok(vm.heap.allocated as u64),
        "get_init_heapsize" => |_, _| Ok(INIT_HEAP),
        "get_max_heapsize" => |_, _| Ok(MAX_HEAP),
        "collect"
        | "init"
        | "add_roots"
        | "remove_roots"
        | "yield"
        | "set_mutator_thread_state"
        | "set_weak_references_collected_callback"
        | "stats_collection_total"
        | "stats_collection_duration_total" => |_, _| Ok(0),
        _ => return None,
    };
    Some(f)
}

/// `alloc(cls, size)`: a zeroed object of `size` bytes whose header points to `cls`.
fn alloc(vm: &mut Vm, args: &[u64]) -> Flow {
    let obj = vm.heap.alloc(args[1] as usize);
    heap::write(obj, args[0]);
    Ok(obj as u64)
}

/// `alloc_array(cls, length, stride)`.
fn alloc_array(vm: &mut Vm, args: &[u64]) -> Flow {
    let (rtti, length, stride) = (args[0] as usize, args[1] as u32 as usize, args[2] as u32 as usize);
    let header = vm.class(vm.class_of_rtti(rtti)).size as usize;
    let arr = vm.heap.alloc(header + length * stride);
    heap::write(arr, rtti);
    heap::write(arr + ARRAY_LENGTH, length as i32);
    heap::write(arr + ARRAY_STRIDE, stride as i32);
    Ok(arr as u64)
}
