//! Raw memory: objects and arrays in Scala Native's Immix collector, a never-freed permanent
//! heap for runtime metadata, modules and literals, and the only place the VM reads or writes
//! memory by address.
//!
//! Addresses come from interpreted NIR, which the VM trusts the way a CPU trusts the machine
//! code it runs: NIR may compute any address, and a wrong one is a bug in that program.
//!
//! The collector finds live objects from the native stack, the frame stack and the permanent
//! heap, all scanned conservatively; see ADR 0004.

use std::alloc::{Layout, alloc_zeroed};
use std::sync::atomic::{AtomicBool, Ordering};

const CHUNK: usize = 1 << 20;
const ALIGN: usize = 16;

pub struct Heap {
    cur: usize,
    end: usize,
    pub allocated: usize,
    gc: Collector,
}

/// The collector's entry points in the C runtime.
struct Collector {
    alloc: extern "C" fn(usize, usize) -> usize,
    alloc_array: extern "C" fn(usize, usize, usize) -> usize,
    add_roots: extern "C" fn(usize, usize),
    set_ids: extern "C" fn(*const GcIds),
    watch_weak_refs: extern "C" fn(),
    set_threads: extern "C" fn(*const ThreadRoots, i32),
    switch_stack: extern "C" fn(usize, usize),
    /// The `int` the collector sets when it has cleared weak references.
    weak_refs_collected: usize,
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
/// apart, and where a weak reference keeps its referent; `scalai_GC_set_ids` reads it.
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
    /// Starts the collector on the calling thread, whose stack spans `stack`. `symbol` finds C
    /// runtime functions.
    pub fn new(symbol: impl Fn(&str) -> Option<usize>, stack: (usize, usize)) -> Result<Self, String> {
        static STARTED: AtomicBool = AtomicBool::new(false);
        if STARTED.swap(true, Ordering::SeqCst) {
            return Err("the collector serves one VM per process".into());
        }
        let find = |name: &str| symbol(name).ok_or_else(|| format!("C runtime lacks {name}"));
        // SAFETY: each address is the named C runtime function, whose C signature the type the
        // result is assigned to spells out.
        let gc = unsafe {
            Collector {
                alloc: c_fn(find("scalanative_GC_alloc")?),
                alloc_array: c_fn(find("scalanative_GC_alloc_array")?),
                add_roots: c_fn(find("scalanative_GC_add_roots")?),
                set_ids: c_fn(find("scalai_GC_set_ids")?),
                watch_weak_refs: c_fn(find("scalai_GC_watch_weak_refs")?),
                set_threads: c_fn(find("scalai_GC_set_threads")?),
                switch_stack: c_fn(find("scalai_GC_switch_stack")?),
                weak_refs_collected: find("scalai_weak_refs_collected")?,
            }
        };
        // SAFETY: as above.
        let init: extern "C" fn(usize, usize) = unsafe { c_fn(find("scalai_GC_init")?) };
        init(stack.0, stack.1);
        Ok(Self { cur: 0, end: 0, allocated: 0, gc })
    }

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

    /// A zeroed object of `size` bytes whose header points to `rtti`; may collect first.
    pub fn object(&mut self, rtti: usize, size: usize) -> usize {
        (self.gc.alloc)(rtti, size)
    }

    /// A zeroed array of `len` elements of `stride` bytes; may collect first.
    pub fn array(&mut self, rtti: usize, len: usize, stride: usize) -> usize {
        (self.gc.alloc_array)(rtti, len, stride)
    }

    /// `size` zeroed bytes aligned to 16 that are never freed; the collector scans them.
    pub fn permanent(&mut self, size: usize) -> usize {
        let size = size.max(1).next_multiple_of(ALIGN);
        self.allocated += size;
        if size > CHUNK / 4 {
            return self.chunk(size);
        }
        if self.cur + size > self.end {
            self.cur = self.chunk(CHUNK);
            self.end = self.cur + CHUNK;
        }
        let addr = self.cur;
        self.cur += size;
        addr
    }

    fn chunk(&mut self, size: usize) -> usize {
        let layout = Layout::from_size_align(size, ALIGN).expect("allocation size overflows");
        // SAFETY: `layout` has a non-zero size.
        let ptr = unsafe { alloc_zeroed(layout) } as usize;
        assert!(ptr != 0, "out of memory allocating {size} bytes");
        (self.gc.add_roots)(ptr, ptr + size);
        ptr
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

/// Addresses below this lie in the page every platform leaves unmapped; accessing them is a
/// VM or program bug that panics instead of faulting.
const NULL_PAGE: usize = 4096;

/// Reads a `T` at `addr`, which the running program vouches for.
pub fn read<T: Copy>(addr: usize) -> T {
    assert!(addr >= NULL_PAGE, "read of address {addr:#x}");
    // SAFETY: the trusted program owns `addr..addr + size_of::<T>()`, see the module
    // documentation and ADR 0002; the read tolerates misalignment.
    unsafe { (addr as *const T).read_unaligned() }
}

/// Copies `len` bytes from `from` to `to`, which the running program vouches for.
pub fn copy(from: usize, to: usize, len: usize) {
    assert!(from >= NULL_PAGE && to >= NULL_PAGE, "copy between {from:#x} and {to:#x}");
    // SAFETY: as for `read`; `copy` tolerates overlapping ranges.
    unsafe { std::ptr::copy(from as *const u8, to as *mut u8, len) }
}

/// Sets `len` bytes at `addr` to zero, which the running program vouches for.
pub fn zero(addr: usize, len: usize) {
    assert!(addr >= NULL_PAGE, "zeroing address {addr:#x}");
    // SAFETY: as for `read`.
    unsafe { std::ptr::write_bytes(addr as *mut u8, 0, len) }
}

/// Writes a `T` at `addr`, which the running program vouches for.
pub fn write<T: Copy>(addr: usize, value: T) {
    assert!(addr >= NULL_PAGE, "write of address {addr:#x}");
    // SAFETY: as for `read`.
    unsafe { (addr as *mut T).write_unaligned(value) }
}
