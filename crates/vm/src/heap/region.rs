//! The heap region at a fixed address, which holds the permanent heap and, while the VM trains,
//! every allocation, each recorded with what it holds; see ADR 0034.

use super::Heap;
use std::ops::Range;

#[cfg(target_os = "macos")]
const HINT: usize = 0x2000_0000_0000;
#[cfg(not(target_os = "macos"))]
const HINT: usize = 0x5000_0000_0000;

const SIZE: usize = 64 << 20;

/// The step in which the region's used prefix becomes a root range of the collector.
const ROOTS_STEP: usize = 1 << 20;

/// What an allocation holds, as the snapshot walk reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blob {
    /// An object or array, typed by its header.
    Object,
    /// Words of which any may be a pointer.
    Words,
    /// Bytes without pointers.
    Data,
    /// A value of the NIR type.
    Typed(nir::TypeId),
}

/// The range training filled and its allocations in address order.
#[derive(Debug, PartialEq)]
pub struct Trained<'a> {
    pub range: Range<usize>,
    pub blobs: &'a [(usize, Blob)],
}

enum Training {
    On(Vec<(usize, Blob)>),
    Done(Vec<(usize, Blob)>, usize),
    Off,
}

pub struct Region {
    start: usize,
    cur: usize,
    end: usize,
    /// The end of the prefix registered as roots.
    rooted: usize,
    training: Training,
}

impl Region {
    /// The region at `HINT`, in training, unless that address is not granted.
    pub fn map() -> Option<Self> {
        let start = super::map(HINT, SIZE)?;
        if start != HINT {
            super::unmap(start, SIZE);
            return None;
        }
        Some(Self { start, cur: start, end: start + SIZE, rooted: start, training: Training::On(Vec::new()) })
    }

    /// `size` zeroed bytes, a multiple of 16, unless the region is full; `add_roots` registers
    /// the range the allocation makes the collector scan.
    pub fn alloc(&mut self, size: usize, blob: Blob, add_roots: impl FnOnce(usize, usize)) -> Option<usize> {
        if size > self.end - self.cur {
            if let Training::On(_) = self.training {
                self.training = Training::Off;
            }
            return None;
        }
        let addr = self.cur;
        self.cur += size;
        if self.cur > self.rooted {
            let to = self.cur.next_multiple_of(ROOTS_STEP).min(self.end);
            add_roots(self.rooted, to);
            self.rooted = to;
        }
        if let Training::On(blobs) = &mut self.training {
            blobs.push((addr, blob));
        }
        Some(addr)
    }

    /// Copies `bytes`, a snapshot of a region at `start`, to the start of this unused one, and
    /// ends training; whether the region is that one.
    pub fn restore(&mut self, start: usize, bytes: &[u8], add_roots: impl FnOnce(usize, usize)) -> bool {
        if start != self.start || self.cur != start || bytes.len() > self.end - start {
            return false;
        }
        self.training = Training::Off;
        self.alloc(bytes.len().next_multiple_of(16), Blob::Data, add_roots).expect("checked size");
        super::copy(bytes.as_ptr() as usize, start, bytes.len());
        true
    }

    /// Zeroes what `restore` copied and trains again, as a freshly mapped region does.
    pub fn forget(&mut self) {
        super::zero(self.start, self.cur - self.start);
        self.cur = self.start;
        self.training = Training::On(Vec::new());
    }

    /// An object of `size` bytes as `alloc` makes it, while training.
    pub fn train(&mut self, size: usize, add_roots: impl FnOnce(usize, usize)) -> Option<usize> {
        match self.training {
            Training::On(_) => self.alloc(size, Blob::Object, add_roots),
            _ => None,
        }
    }

    /// Ends training; `keep` keeps what it allocated as a snapshot.
    pub fn end_training(&mut self, keep: bool) {
        self.training = match std::mem::replace(&mut self.training, Training::Off) {
            Training::On(blobs) if keep => Training::Done(blobs, self.cur),
            other @ Training::Done(..) => other,
            _ => Training::Off,
        };
    }

    /// What training filled, once it has ended and was kept.
    pub fn trained(&self) -> Option<Trained<'_>> {
        match &self.training {
            Training::Done(blobs, end) => Some(Trained { range: self.start..*end, blobs }),
            _ => None,
        }
    }
}

impl Heap {
    /// Sends objects and arrays to the collected heap from now on; `keep` keeps what training
    /// allocated as a snapshot.
    pub fn end_training(&mut self, keep: bool) {
        if let Some(region) = &mut self.region {
            region.end_training(keep);
        }
    }

    pub fn trained(&self) -> Option<Trained<'_>> {
        self.region.as_ref()?.trained()
    }

    /// Puts snapshot `bytes` taken at `start` back in place; whether that succeeded.
    pub fn restore(&mut self, start: usize, bytes: &[u8]) -> bool {
        let add_roots = self.gc.add_roots;
        self.region.as_mut().is_some_and(|r| r.restore(start, bytes, |low, high| add_roots(low, high)))
    }

    /// Takes back a restored snapshot.
    pub fn forget_snapshot(&mut self) {
        if let Some(region) = &mut self.region {
            region.forget();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granted_at_hint_and_filled_in_order() {
        let mut region = Region::map().expect("region hint not granted");
        let mut roots = Vec::new();
        let a = region.train(16, |lo, hi| roots.push((lo, hi))).unwrap();
        let b = region.alloc(ROOTS_STEP, Blob::Data, |lo, hi| roots.push((lo, hi))).unwrap();
        assert_eq!((a, b), (HINT, HINT + 16));
        assert_eq!(roots, [(HINT, HINT + ROOTS_STEP), (HINT + ROOTS_STEP, HINT + 2 * ROOTS_STEP)]);
        assert_eq!(crate::heap::read::<u64>(b + ROOTS_STEP - 8), 0);
        region.end_training(true);
        let blobs = [(a, Blob::Object), (b, Blob::Data)];
        assert_eq!(region.trained(), Some(Trained { range: HINT..b + ROOTS_STEP, blobs: &blobs }));
        assert!(region.train(16, |_, _| ()).is_none());
        assert!(region.alloc(SIZE, Blob::Data, |_, _| ()).is_none());
    }
}
