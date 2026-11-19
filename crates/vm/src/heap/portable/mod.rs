//! The collected heap in portable mode (ADR 0127): objects bumped into the free runs of aligned
//! chunks, large ones from the allocator, and a non-moving mark-sweep that scans the permanent
//! heap, registered roots, the frame stacks and every reached object conservatively. It runs
//! only at safepoints the VM chooses, where no Rust code holds an object it does not root.

mod chunks;
mod mark;

use super::{GcIds, Heap, ThreadRoots, write};
use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE};
use chunks::{CHUNK, Chunks, GRANULE, Span};
use std::collections::BTreeMap;

/// Default bytes of live objects and arrays past which allocation throws `OutOfMemoryError`.
pub const HEAP_CAP: usize = 1 << 30;

/// Bytes allocated past the cap between two `OutOfMemoryError`s, room for constructing one.
const OUT_OF_MEMORY_STEP: usize = 16 << 20;

/// Bytes allocated before the first collection, and at least between two.
const MIN_TRIGGER: usize = 8 << 20;

/// Objects larger than this get memory of their own.
const LARGE: usize = CHUNK / 4;

pub struct Collector {
    chunks: Chunks,
    /// Large objects by address: size and mark.
    large: BTreeMap<usize, (usize, bool)>,
    /// The free run allocation bumps into, and the runs after it.
    cur: Span,
    holes: Vec<Span>,
    /// Ranges scanned as roots: the permanent heap and what the program registers.
    roots: Vec<Span>,
    /// The `[low, top]` cells of every thread's frame stack.
    frames: Vec<usize>,
    used: usize,
    cap: usize,
    next_gc: usize,
    raise_at: usize,
    over_cap: bool,
    /// Code running that may hold objects only in Rust locals: no collection then.
    pins: u32,
}

impl Collector {
    /// A heap with `HEAP_CAP`; `symbol` is not needed.
    pub fn new(_symbol: impl Fn(&str) -> Option<usize>) -> Result<Self, String> {
        let mut gc = Self {
            chunks: Chunks::default(),
            large: BTreeMap::new(),
            cur: Span { addr: 0, len: 0 },
            holes: Vec::new(),
            roots: Vec::new(),
            frames: Vec::new(),
            used: 0,
            cap: 0,
            next_gc: 0,
            raise_at: 0,
            over_cap: false,
            pins: 0,
        };
        gc.set_cap(HEAP_CAP);
        Ok(gc)
    }

    fn set_cap(&mut self, cap: usize) {
        self.cap = cap;
        self.next_gc = self.used + MIN_TRIGGER.min(cap / 2);
        self.raise_at = cap.max(self.used) + OUT_OF_MEMORY_STEP;
    }

    /// Starts nothing: no native stack is scanned.
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

    /// Scans `low..high` as roots from now on.
    pub fn add_roots(&mut self, low: usize, high: usize) {
        self.roots.push(Span { addr: low, len: high - low });
    }

    /// `size` zeroed bytes aligned to `GRANULE`, counted against the cap.
    fn take(&mut self, size: usize) -> usize {
        let size = size.max(1).checked_next_multiple_of(GRANULE).filter(|&s| s <= self.cap);
        let Some(size) = size else { panic!("java.lang.OutOfMemoryError: an array beyond the heap's cap") };
        self.used += size;
        if self.used > self.raise_at {
            self.over_cap = true;
            self.raise_at = self.used + OUT_OF_MEMORY_STEP;
        }
        if size > LARGE {
            let layout = std::alloc::Layout::from_size_align(size, GRANULE).expect("a valid layout");
            // SAFETY: `layout` has a non-zero size.
            let addr = unsafe { std::alloc::alloc_zeroed(layout) } as usize;
            assert!(addr != 0, "java.lang.OutOfMemoryError: the host refused {size} bytes");
            self.large.insert(addr, (size, false));
            return addr;
        }
        while self.cur.len < size {
            self.cur = match self.holes.pop() {
                Some(hole) => hole,
                None => Span { addr: self.chunks.add(), len: CHUNK },
            };
        }
        let addr = self.cur.addr;
        self.cur = Span { addr: addr + size, len: self.cur.len - size };
        super::zero(addr, size);
        self.chunks.record(addr, size);
        addr
    }
}

impl Heap {
    /// Ignores `ids`: the collector tells objects apart by its own records.
    pub fn set_ids(&self, _ids: &GcIds) {}

    /// Makes collections scan the frame stacks of `threads`, whose bounds stay in place until
    /// the next call.
    pub fn set_threads(&mut self, threads: &[ThreadRoots]) {
        self.gc.frames = threads.iter().map(|t| t.frames as usize).collect();
    }

    /// Ignores the native stack: collections never need it.
    pub fn switch_stack(&self, _stack: (usize, usize)) {}

    /// Watches nothing: weak references stay strong.
    pub fn watch_weak_refs(&self) {}

    /// Returns `false`: no collection clears weak references.
    pub fn take_weak_refs_collected(&mut self) -> bool {
        false
    }

    /// Bytes of objects and arrays allocated and not collected.
    pub fn used(&self) -> usize {
        self.gc.used
    }

    /// Makes `cap` the bytes past which allocation throws `OutOfMemoryError`.
    pub fn set_cap(&mut self, cap: usize) {
        self.gc.set_cap(cap);
    }

    /// Whether allocations passed the cap since the last call.
    pub fn take_over_cap(&mut self) -> bool {
        std::mem::take(&mut self.gc.over_cap)
    }

    /// Scans `low..high` as roots from now on.
    pub fn add_roots(&mut self, low: usize, high: usize) {
        self.gc.add_roots(low, high);
    }

    /// Forbids collections until the matching `unpin`.
    pub fn pin(&mut self) {
        self.gc.pins += 1;
    }

    pub fn unpin(&mut self) {
        self.gc.pins -= 1;
    }

    /// Collects if enough was allocated since the last collection and nothing pins the heap.
    pub fn collect_if_due(&mut self) {
        if self.gc.pins == 0 && self.gc.used >= self.gc.next_gc {
            self.gc.collect();
        }
    }
}
