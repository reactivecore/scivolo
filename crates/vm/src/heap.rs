//! Raw memory: objects and arrays in Scala Native's Immix collector, a never-freed permanent
//! heap for runtime metadata, modules and literals, and the only place the VM reads or writes
//! memory by address.
//!
//! Addresses come from interpreted NIR, which the VM trusts the way a CPU trusts the machine
//! code it runs: NIR may compute any address, and a wrong one is a bug in that program.
//!
//! The collector finds live objects from the native stack, the frame stack and the permanent
//! heap, all scanned conservatively; see ADR 0004.

mod collector;
mod region;

pub use collector::{GcIds, ThreadRoots};

use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE};
use collector::Collector;
use region::Region;
use std::alloc::{Layout, alloc_zeroed};
use std::sync::atomic::{AtomicBool, Ordering};

const CHUNK: usize = 1 << 20;
const ALIGN: usize = 16;

pub struct Heap {
    cur: usize,
    end: usize,
    pub allocated: usize,
    region: Option<Region>,
    /// Whether objects and arrays go to the region instead of the collected heap.
    training: bool,
    gc: Collector,
}

impl Heap {
    /// Starts the collector on the calling thread, whose stack spans `stack`, in training if
    /// the region is mapped. `symbol` finds C runtime functions.
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
                set_ids: c_fn(find("scivolo_GC_set_ids")?),
                watch_weak_refs: c_fn(find("scivolo_GC_watch_weak_refs")?),
                set_threads: c_fn(find("scivolo_GC_set_threads")?),
                switch_stack: c_fn(find("scivolo_GC_switch_stack")?),
                weak_refs_collected: find("scivolo_weak_refs_collected")?,
            }
        };
        // SAFETY: as above.
        let init: extern "C" fn(usize, usize) = unsafe { c_fn(find("scivolo_GC_init")?) };
        let region = Region::map();
        init(stack.0, stack.1);
        let training = region.is_some();
        Ok(Self { cur: 0, end: 0, allocated: 0, region, training, gc })
    }

    /// Sends objects and arrays to the collected heap from now on.
    pub fn end_training(&mut self) {
        self.training = false;
    }

    /// A zeroed object of `size` bytes whose header points to `rtti`; may collect first.
    pub fn object(&mut self, rtti: usize, size: usize) -> usize {
        if self.training
            && let Some(obj) = self.in_region(size.next_multiple_of(ALIGN))
        {
            write(obj, rtti);
            return obj;
        }
        (self.gc.alloc)(rtti, size)
    }

    /// A zeroed array of `len` elements of `stride` bytes; may collect first.
    pub fn array(&mut self, rtti: usize, len: usize, stride: usize) -> usize {
        if self.training {
            let size = ARRAY_HEADER as usize + len * stride;
            if let Some(arr) = self.in_region(size.next_multiple_of(ALIGN)) {
                write(arr, rtti);
                write(arr + ARRAY_LENGTH, len as i32);
                write(arr + ARRAY_STRIDE, stride as i32);
                return arr;
            }
        }
        (self.gc.alloc_array)(rtti, len, stride)
    }

    /// `size` zeroed bytes aligned to 16 that are never freed; the collector scans them.
    pub fn permanent(&mut self, size: usize) -> usize {
        let size = size.max(1).next_multiple_of(ALIGN);
        self.allocated += size;
        if let Some(addr) = self.in_region(size) {
            return addr;
        }
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

    fn in_region(&mut self, size: usize) -> Option<usize> {
        let add_roots = self.gc.add_roots;
        self.region.as_mut()?.alloc(size, |low, high| add_roots(low, high))
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

/// `len` zeroed bytes of fresh address space, at `hint` if that is free.
fn map(hint: usize, len: usize) -> Option<usize> {
    // SAFETY: an anonymous private mapping without `MAP_FIXED` touches no existing memory.
    let addr = unsafe {
        libc::mmap(
            hint as *mut libc::c_void,
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_NORESERVE,
            -1,
            0,
        )
    };
    (addr != libc::MAP_FAILED).then_some(addr as usize)
}

/// Releases `len` bytes `map` returned at `addr`, of which nothing was handed out.
fn unmap(addr: usize, len: usize) {
    // SAFETY: the caller's contract; nothing refers to the mapping.
    unsafe { libc::munmap(addr as *mut libc::c_void, len) };
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
