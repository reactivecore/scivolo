//! The collector's entry points in the C runtime, and the VM's calls through them that are not
//! allocations.

use super::{GcIds, Heap, ThreadRoots, read, write};

pub struct Collector {
    alloc: extern "C" fn(usize, usize) -> usize,
    alloc_array: extern "C" fn(usize, usize, usize) -> usize,
    add_roots: extern "C" fn(usize, usize),
    set_ids: extern "C" fn(*const GcIds),
    watch_weak_refs: extern "C" fn(),
    set_threads: extern "C" fn(*const ThreadRoots, i32),
    switch_stack: extern "C" fn(usize, usize),
    /// The `int` the collector sets when it has cleared weak references.
    weak_refs_collected: usize,
    init: extern "C" fn(usize, usize),
}

impl Collector {
    /// Finds Immix's entry points with `symbol`, which finds C runtime functions.
    pub fn new(symbol: impl Fn(&str) -> Option<usize>) -> Result<Self, String> {
        let find = |name: &str| symbol(name).ok_or_else(|| format!("C runtime lacks {name}"));
        // SAFETY: each address is the named C runtime function, whose C signature the type the
        // result is assigned to spells out.
        unsafe {
            Ok(Collector {
                init: c_fn(find("scivolo_GC_init")?),
                alloc: c_fn(find("scalanative_GC_alloc")?),
                alloc_array: c_fn(find("scalanative_GC_alloc_array")?),
                add_roots: c_fn(find("scalanative_GC_add_roots")?),
                set_ids: c_fn(find("scivolo_GC_set_ids")?),
                watch_weak_refs: c_fn(find("scivolo_GC_watch_weak_refs")?),
                set_threads: c_fn(find("scivolo_GC_set_threads")?),
                switch_stack: c_fn(find("scivolo_GC_switch_stack")?),
                weak_refs_collected: find("scivolo_weak_refs_collected")?,
            })
        }
    }

    /// Starts Immix on the calling thread, whose stack spans `stack`.
    pub fn start(&self, stack: (usize, usize)) {
        (self.init)(stack.0, stack.1);
    }

    /// A zeroed object of `size` bytes whose header points to `rtti`; may collect first.
    pub fn alloc(&mut self, rtti: usize, size: usize) -> usize {
        (self.alloc)(rtti, size)
    }

    /// A zeroed array of `len` elements of `stride` bytes; may collect first.
    pub fn alloc_array(&mut self, rtti: usize, len: usize, stride: usize) -> usize {
        (self.alloc_array)(rtti, len, stride)
    }

    /// Makes the collector scan `low..high` conservatively.
    pub fn add_roots(&mut self, low: usize, high: usize) {
        (self.add_roots)(low, high)
    }
}

/// The C function at `addr` as the function pointer type `F`.
///
/// # Safety
/// `F` must be an `extern "C" fn` type matching the function's C signature.
unsafe fn c_fn<F: Copy>(addr: usize) -> F {
    assert_eq!(size_of::<F>(), size_of::<usize>());
    // SAFETY: `F` is a function pointer, as large as `addr`, per the caller's contract.
    unsafe { std::mem::transmute_copy(&addr) }
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
