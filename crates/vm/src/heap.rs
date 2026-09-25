//! Raw, zeroed, never-freed memory for objects, arrays and runtime metadata.

use std::alloc::{Layout, alloc_zeroed};

const CHUNK: usize = 1 << 20;
const ALIGN: usize = 16;

#[derive(Default)]
pub struct Heap {
    cur: usize,
    end: usize,
    pub allocated: usize,
}

impl Heap {
    /// `size` zeroed bytes aligned to 16.
    pub fn alloc(&mut self, size: usize) -> usize {
        let size = size.max(1).next_multiple_of(ALIGN);
        self.allocated += size;
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

fn chunk(size: usize) -> usize {
    let layout = Layout::from_size_align(size, ALIGN).expect("allocation size overflows");
    // SAFETY: `layout` has a non-zero size.
    let ptr = unsafe { alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "out of memory allocating {size} bytes");
    ptr as usize
}

/// Reads a `T` at `addr`.
///
/// # Safety
/// `addr..addr + size_of::<T>()` must be readable memory.
pub unsafe fn load<T: Copy>(addr: usize) -> T {
    // SAFETY: guaranteed by the caller; the read tolerates misalignment.
    unsafe { (addr as *const T).read_unaligned() }
}

/// Writes a `T` at `addr`.
///
/// # Safety
/// `addr..addr + size_of::<T>()` must be writable memory.
pub unsafe fn store<T: Copy>(addr: usize, value: T) {
    // SAFETY: guaranteed by the caller; the write tolerates misalignment.
    unsafe { (addr as *mut T).write_unaligned(value) }
}
