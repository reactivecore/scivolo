//! A collection: marking from the roots, conservatively, then sweeping what stayed unmarked.

use super::{Collector, MIN_TRIGGER, OUT_OF_MEMORY_STEP, Span};
use crate::heap::read;
use std::alloc::{Layout, dealloc};

impl Collector {
    /// Frees every object no root reaches, then sets when the next collection is due and
    /// whether the live objects exceed the cap.
    pub fn collect(&mut self) {
        let mut work = Vec::new();
        for i in 0..self.roots.len() {
            let root = self.roots[i];
            self.scan(root, &mut work);
        }
        for i in 0..self.frames.len() {
            let cell = self.frames[i];
            let (low, top) = (read::<usize>(cell), read::<usize>(cell + size_of::<usize>()));
            self.scan(Span { addr: low, len: top - low }, &mut work);
        }
        while let Some(object) = work.pop() {
            self.scan(object, &mut work);
        }
        self.sweep();
    }

    /// Marks every unmarked object a word of `span` points into, and queues it for scanning.
    fn scan(&mut self, span: Span, work: &mut Vec<Span>) {
        let mut at = span.addr.next_multiple_of(8);
        while at + 8 <= span.addr + span.len {
            if let Ok(word) = usize::try_from(read::<u64>(at))
                && let Some(object) = self.mark(word)
            {
                work.push(object);
            }
            at += 8;
        }
    }

    fn mark(&mut self, addr: usize) -> Option<Span> {
        if let Some(object) = self.chunks.mark(addr) {
            return Some(object);
        }
        let (&start, (size, marked)) = self.large.range_mut(..=addr).next_back()?;
        if addr >= start + *size || *marked {
            return None;
        }
        *marked = true;
        Some(Span { addr: start, len: *size })
    }

    fn sweep(&mut self) {
        self.holes.clear();
        self.cur = Span { addr: 0, len: 0 };
        let mut live = self.chunks.sweep(&mut self.holes);
        self.large.retain(|&addr, (size, marked)| {
            if std::mem::take(marked) {
                live += *size;
                return true;
            }
            let layout =
                Layout::from_size_align(*size, super::GRANULE).expect("the layout it was allocated with");
            // SAFETY: `addr` came from `alloc_zeroed` with this layout, and no root reaches it.
            unsafe { dealloc(addr as *mut u8, layout) };
            false
        });
        self.used = live;
        if live > self.cap {
            self.over_cap = true;
            self.next_gc = live + OUT_OF_MEMORY_STEP;
            self.raise_at = live + OUT_OF_MEMORY_STEP;
            return;
        }
        let next = (2 * live).max(live + MIN_TRIGGER.min(self.cap / 2));
        self.next_gc = next.min(self.cap).max(live + 1);
        self.raise_at = self.cap + OUT_OF_MEMORY_STEP;
    }
}
