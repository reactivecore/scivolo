//! The collector's C interface (`scalanative_GC_*`) in portable mode, on the VM's own heap,
//! which collects at its own safepoints only; root ranges are never removed and weak references
//! stay strong.

use crate::heap::{self, HEAP_CAP};
use crate::natives::NativeFn;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name.strip_prefix("scalanative_GC_")? {
        "alloc" | "alloc_small" | "alloc_large" => {
            |vm, a| Ok(vm.heap.object(a[0] as usize, a[1] as usize) as u64)
        }
        "alloc_array" => |vm, a| Ok(vm.heap.array(a[0] as usize, a[1] as usize, a[2] as usize) as u64),
        "weak_ref_slot_create" => |vm, a| {
            let slot = vm.heap.permanent(8, heap::Blob::Words);
            heap::write(slot, a[0]);
            Ok(slot as u64)
        },
        "weak_ref_slot_get" => |_, a| Ok(heap::read(a[0] as usize)),
        "weak_ref_slot_clear" => |_, a| {
            heap::write(a[0] as usize, 0u64);
            Ok(0)
        },
        "get_init_heapsize" | "get_max_heapsize" => |_, _| Ok(HEAP_CAP as u64),
        "get_used_heapsize" => |vm, _| Ok(vm.heap.used() as u64),
        "add_roots" => |vm, a| {
            vm.heap.add_roots(a[0] as usize, a[1] as usize);
            Ok(0)
        },
        "remove_roots"
        | "collect"
        | "set_mutator_thread_state"
        | "yield"
        | "stats_collection_total"
        | "stats_collection_duration_total" => |_, _| Ok(0),
        _ => return None,
    };
    Some(f)
}
