//! The chunks small objects live in: aligned blocks of the Rust allocator with a bitmap of
//! object starts, one of object ends and one of marks, one bit per granule.

use rustc_hash::FxHashMap;
use std::alloc::{Layout, alloc_zeroed};

/// Bytes of a chunk, and its alignment, so an address finds its chunk by masking.
pub const CHUNK: usize = 1 << 20;
/// The unit objects are aligned to and sized in.
pub const GRANULE: usize = 16;
const GRANULES: usize = CHUNK / GRANULE;
const WORDS: usize = GRANULES / 64;

type Bits = Box<[u64; WORDS]>;

struct Chunk {
    base: usize,
    starts: Bits,
    ends: Bits,
    marks: Bits,
}

/// A run of memory, an object or a free one: its address and length.
#[derive(Clone, Copy)]
pub struct Span {
    pub addr: usize,
    pub len: usize,
}

#[derive(Default)]
pub struct Chunks {
    all: Vec<Chunk>,
    by_base: FxHashMap<usize, usize>,
}

fn bits() -> Bits {
    Box::new([0; WORDS])
}

fn get(b: &Bits, g: usize) -> bool {
    b[g / 64] & (1 << (g % 64)) != 0
}

fn set(b: &mut Bits, g: usize, on: bool) {
    match on {
        true => b[g / 64] |= 1 << (g % 64),
        false => b[g / 64] &= !(1 << (g % 64)),
    }
}

impl Chunks {
    /// Adds a zeroed chunk and returns its base.
    pub fn add(&mut self) -> usize {
        let layout = Layout::from_size_align(CHUNK, CHUNK).expect("a valid layout");
        // SAFETY: `layout` has a non-zero size.
        let base = unsafe { alloc_zeroed(layout) } as usize;
        assert!(base != 0, "java.lang.OutOfMemoryError: the host refused a {CHUNK}-byte chunk");
        self.by_base.insert(base, self.all.len());
        self.all.push(Chunk { base, starts: bits(), ends: bits(), marks: bits() });
        base
    }

    /// Records an object of `size` bytes, a multiple of `GRANULE`, at `addr` in a chunk.
    pub fn record(&mut self, addr: usize, size: usize) {
        let (c, g) = self.locate(addr).expect("an address in a chunk");
        let chunk = &mut self.all[c];
        set(&mut chunk.starts, g, true);
        set(&mut chunk.ends, g + size / GRANULE - 1, true);
    }

    fn locate(&self, addr: usize) -> Option<(usize, usize)> {
        let c = *self.by_base.get(&(addr & !(CHUNK - 1)))?;
        Some((c, (addr - self.all[c].base) / GRANULE))
    }

    /// The object `addr` points into, as its chunk, first granule and last granule.
    fn object(&self, addr: usize) -> Option<(usize, usize, usize)> {
        let (c, g) = self.locate(addr)?;
        let chunk = &self.all[c];
        let start = last_at_or_before(&chunk.starts, g)?;
        let end = first_at_or_after(&chunk.ends, start)?;
        (g <= end).then_some((c, start, end))
    }

    /// Marks the object `addr` points into; its address and size if it was not marked before.
    pub fn mark(&mut self, addr: usize) -> Option<Span> {
        let (c, start, end) = self.object(addr)?;
        let chunk = &mut self.all[c];
        if get(&chunk.marks, start) {
            return None;
        }
        set(&mut chunk.marks, start, true);
        Some(Span { addr: chunk.base + start * GRANULE, len: (end + 1 - start) * GRANULE })
    }

    /// Forgets the unmarked objects, clears the marks, appends the free runs to `holes` and
    /// returns the bytes of the objects that stay.
    pub fn sweep(&mut self, holes: &mut Vec<Span>) -> usize {
        let mut live = 0;
        for chunk in &mut self.all {
            let mut free_from = 0;
            for w in 0..WORDS {
                let mut starts = chunk.starts[w];
                while starts != 0 {
                    let start = w * 64 + starts.trailing_zeros() as usize;
                    starts &= starts - 1;
                    let end = first_at_or_after(&chunk.ends, start).expect("an end per start");
                    if !get(&chunk.marks, start) {
                        set(&mut chunk.starts, start, false);
                        set(&mut chunk.ends, end, false);
                        continue;
                    }
                    if start > free_from {
                        holes.push(Span {
                            addr: chunk.base + free_from * GRANULE,
                            len: (start - free_from) * GRANULE,
                        });
                    }
                    live += (end + 1 - start) * GRANULE;
                    free_from = end + 1;
                }
            }
            if free_from < GRANULES {
                holes.push(Span {
                    addr: chunk.base + free_from * GRANULE,
                    len: (GRANULES - free_from) * GRANULE,
                });
            }
            chunk.marks.fill(0);
        }
        live
    }
}

/// The highest set bit of `b` at or below `g`.
fn last_at_or_before(b: &Bits, g: usize) -> Option<usize> {
    let mut w = g / 64;
    let mut word = b[w] & (u64::MAX >> (63 - g % 64));
    loop {
        if word != 0 {
            return Some(w * 64 + 63 - word.leading_zeros() as usize);
        }
        w = w.checked_sub(1)?;
        word = b[w];
    }
}

/// The lowest set bit of `b` at or above `g`.
fn first_at_or_after(b: &Bits, g: usize) -> Option<usize> {
    let mut w = g / 64;
    let mut word = b[w] & (u64::MAX << (g % 64));
    loop {
        if word != 0 {
            return Some(w * 64 + word.trailing_zeros() as usize);
        }
        w += 1;
        word = *b.get(w)?;
    }
}
