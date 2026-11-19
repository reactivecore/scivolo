//! What the VM tells the collector: the roots of green threads and the class ids it needs.

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
