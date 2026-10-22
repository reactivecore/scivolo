//! The collector's entry points in the C runtime, and the VM's calls through them that are not
//! allocations.

use super::{Heap, read, write};

pub struct Collector {
    pub alloc: extern "C" fn(usize, usize) -> usize,
    pub alloc_array: extern "C" fn(usize, usize, usize) -> usize,
    pub add_roots: extern "C" fn(usize, usize),
    pub set_ids: extern "C" fn(*const GcIds),
    pub watch_weak_refs: extern "C" fn(),
    pub set_threads: extern "C" fn(*const ThreadRoots, i32),
    pub switch_stack: extern "C" fn(usize, usize),
    /// The `int` the collector sets when it has cleared weak references.
    pub weak_refs_collected: usize,
}

/// The roots of one green thread: its frame stack's `[low, top]` bounds, and `low..high` of its
/// native stack while suspended (empty while it runs).
#[repr(C)]
pub struct ThreadRoots {
    pub frames: *const usize,
    pub low: usize,
    pub high: usize,
}

/// Class ids the collector needs to tell arrays, blobs, boxed pointers and weak references
/// apart, and where a weak reference keeps its referent; `scivolo_GC_set_ids` reads it.
#[repr(C)]
pub struct GcIds {
    pub object_array: i32,
    pub blob_array: i32,
    pub arrays_min: i32,
    pub arrays_max: i32,
    pub boxed_ptr: i32,
    pub weak_refs_min: i32,
    pub weak_refs_max: i32,
    pub weak_referent_offset: i32,
}

impl Heap {
    pub fn set_ids(&self, ids: &GcIds) {
        (self.gc.set_ids)(ids);
    }

    /// Makes the collector scan `threads`, which must stay in place until the next call.
    pub fn set_threads(&self, threads: &[ThreadRoots]) {
        (self.gc.set_threads)(threads.as_ptr(), threads.len() as i32);
    }

    /// Makes the collector scan the running code's native stack within `low..high`.
    pub fn switch_stack(&self, (low, high): (usize, usize)) {
        (self.gc.switch_stack)(low, high);
    }

    /// Makes collections that clear weak references raise a flag instead of calling back.
    pub fn watch_weak_refs(&self) {
        (self.gc.watch_weak_refs)()
    }

    /// Whether a collection cleared weak references since the last call.
    pub fn take_weak_refs_collected(&mut self) -> bool {
        let flag = self.gc.weak_refs_collected;
        let raised = read::<i32>(flag) != 0;
        if raised {
            write(flag, 0i32);
        }
        raised
    }
}
