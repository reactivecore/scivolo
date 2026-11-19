//! The collected heap in portable mode: objects bumped from chunks of the Rust allocator and
//! never freed, up to a cap past which the running code gets an `OutOfMemoryError`.

use super::{ALIGN, CHUNK, GcIds, Heap, ThreadRoots, write};
use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE};
use std::alloc::{Layout, alloc_zeroed};

/// Bytes of objects and arrays a portable VM allocates before `OutOfMemoryError`s start.
pub const HEAP_CAP: usize = 1 << 30;

/// Bytes allocated past the cap between two `OutOfMemoryError`s, room for constructing one.
const OUT_OF_MEMORY_STEP: usize = 16 << 20;

pub struct Collector {
    cur: usize,
    end: usize,
    used: usize,
    /// The use past which `over_cap` is raised next.
    raise_at: usize,
    over_cap: bool,
}

impl Collector {
    /// A heap without a collector; `symbol` is not needed.
    pub fn new(_symbol: impl Fn(&str) -> Option<usize>) -> Result<Self, String> {
        Ok(Self { cur: 0, end: 0, used: 0, raise_at: HEAP_CAP, over_cap: false })
    }

    /// Starts nothing: no stack is scanned.
    pub fn start(&self, _stack: (usize, usize)) {}

    /// A zeroed object of `size` bytes whose header points to `rtti`.
    pub fn alloc(&mut self, rtti: usize, size: usize) -> usize {
        let obj = self.take(size);
        write(obj, rtti);
        obj
    }

    /// A zeroed array of `len` elements of `stride` bytes.
    pub fn alloc_array(&mut self, rtti: usize, len: usize, stride: usize) -> usize {
        let size = len.checked_mul(stride).and_then(|s| s.checked_add(ARRAY_HEADER as usize));
        let arr = self.take(size.unwrap_or(usize::MAX));
        write(arr, rtti);
        write(arr + ARRAY_LENGTH, len as i32);
        write(arr + ARRAY_STRIDE, stride as i32);
        arr
    }

    /// Scans nothing: nothing is collected.
    pub fn add_roots(&self, _low: usize, _high: usize) {}

    /// `size` zeroed bytes aligned to `ALIGN`; past `HEAP_CAP`, and every `OUT_OF_MEMORY_STEP`
    /// after, it raises `over_cap`.
    fn take(&mut self, size: usize) -> usize {
        let size = size.max(1).checked_next_multiple_of(ALIGN).filter(|&s| s <= HEAP_CAP);
        let Some(size) = size else { panic!("java.lang.OutOfMemoryError: an array beyond {HEAP_CAP} bytes") };
        self.used += size;
        if self.used > self.raise_at {
            self.over_cap = true;
            self.raise_at = self.used + OUT_OF_MEMORY_STEP;
        }
        if size > CHUNK / 4 {
            return chunk(size);
        }
        if self.cur + size > self.end {
            self.cur = chunk(CHUNK);
            self.end = self.cur + CHUNK;
        }
        let addr = self.cur;
        self.cur += size;
        addr
    }
}

/// `size` zeroed bytes from the allocator, never freed.
fn chunk(size: usize) -> usize {
    let layout = Layout::from_size_align(size, ALIGN).expect("allocation size overflows");
    // SAFETY: `layout` has a non-zero size.
    let ptr = unsafe { alloc_zeroed(layout) } as usize;
    assert!(ptr != 0, "java.lang.OutOfMemoryError: the host refused {size} bytes");
    ptr
}

impl Heap {
    /// Ignores `ids`: no collection tells objects apart.
    pub fn set_ids(&self, _ids: &GcIds) {}

    /// Ignores `threads`: no collection scans them.
    pub fn set_threads(&self, _threads: &[ThreadRoots]) {}

    /// Ignores the stack: no collection scans it.
    pub fn switch_stack(&self, _stack: (usize, usize)) {}

    /// Watches nothing: no collection clears weak references.
    pub fn watch_weak_refs(&self) {}

    /// Returns `false`: no collection clears weak references.
    pub fn take_weak_refs_collected(&mut self) -> bool {
        false
    }

    /// Bytes of objects and arrays allocated so far.
    pub fn used(&self) -> usize {
        self.gc.used
    }

    /// Whether allocations passed `HEAP_CAP` since the last call.
    pub fn take_over_cap(&mut self) -> bool {
        std::mem::take(&mut self.gc.over_cap)
    }
}
