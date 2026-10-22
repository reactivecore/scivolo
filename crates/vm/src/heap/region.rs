//! The heap region at a fixed address, which holds the permanent heap and, while the VM trains,
//! every allocation; see ADR 0034.

#[cfg(target_os = "macos")]
const HINT: usize = 0x2000_0000_0000;
#[cfg(not(target_os = "macos"))]
const HINT: usize = 0x5000_0000_0000;

const SIZE: usize = 64 << 20;

/// The step in which the region's used prefix becomes a root range of the collector.
const ROOTS_STEP: usize = 1 << 20;

pub struct Region {
    cur: usize,
    end: usize,
    /// The end of the prefix registered as roots.
    rooted: usize,
}

impl Region {
    /// The region at `HINT`, unless that address is not granted.
    pub fn map() -> Option<Self> {
        let start = super::map(HINT, SIZE)?;
        if start != HINT {
            super::unmap(start, SIZE);
            return None;
        }
        Some(Self { cur: start, end: start + SIZE, rooted: start })
    }

    /// `size` zeroed bytes, a multiple of 16, unless the region is full; `add_roots` registers
    /// the range the allocation makes the collector scan.
    pub fn alloc(&mut self, size: usize, add_roots: impl FnOnce(usize, usize)) -> Option<usize> {
        if size > self.end - self.cur {
            return None;
        }
        let addr = self.cur;
        self.cur += size;
        if self.cur > self.rooted {
            let to = self.cur.next_multiple_of(ROOTS_STEP).min(self.end);
            add_roots(self.rooted, to);
            self.rooted = to;
        }
        Some(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granted_at_hint_and_filled_in_order() {
        let mut region = Region::map().expect("region hint not granted");
        let mut roots = Vec::new();
        let a = region.alloc(16, |lo, hi| roots.push((lo, hi))).unwrap();
        let b = region.alloc(ROOTS_STEP, |lo, hi| roots.push((lo, hi))).unwrap();
        assert_eq!((a, b), (HINT, HINT + 16));
        assert_eq!(roots, [(HINT, HINT + ROOTS_STEP), (HINT + ROOTS_STEP, HINT + 2 * ROOTS_STEP)]);
        assert_eq!(crate::heap::read::<u64>(b + ROOTS_STEP - 8), 0);
        assert!(region.alloc(SIZE, |_, _| ()).is_none());
    }
}
