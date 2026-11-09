// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeBuffer.scala
//   compiler/src/dotty/tools/dotc/util/Util.scala
use super::buffer::{ADDR_WIDTH, Addr, Buffer, nat_size};
use rustc_hash::FxHashMap;

/// A buffer for the `ASTs` section: references go into 4-byte fields that `compactify` shrinks.
#[derive(Debug, Default)]
pub struct TreeBuffer {
    pub buf: Buffer,
    offsets: Vec<usize>,
    is_relative: Vec<bool>,
    tree_addrs: FxHashMap<u32, Addr>,
}

impl TreeBuffer {
    /// The address of tree `id`, set to the current address on its first registration.
    pub fn register_tree(&mut self, id: u32) -> Addr {
        *self.tree_addrs.entry(id).or_insert(Addr(self.buf.len()))
    }

    pub fn addr_of_tree(&self, id: u32) -> Option<Addr> {
        self.tree_addrs.get(&id).copied()
    }

    /// The trees registered, each its id and address.
    pub fn trees(&self) -> impl Iterator<Item = (u32, Addr)> + '_ {
        self.tree_addrs.iter().map(|(&id, &a)| (id, a))
    }

    fn keep_offset(&mut self, relative: bool) {
        self.offsets.push(self.buf.len());
        self.is_relative.push(relative);
    }

    /// Reserves a reference field for `fill_ref`.
    pub fn reserve_ref(&mut self, relative: bool) -> Addr {
        self.keep_offset(relative);
        self.buf.reserve_addr()
    }

    pub fn write_ref(&mut self, target: Addr) {
        self.keep_offset(false);
        let at = self.buf.reserve_addr();
        self.buf.fill_addr(at, target);
    }

    pub fn fill_ref(&mut self, at: Addr, target: Addr, relative: bool) {
        let addr = if relative { target.relative_to(at) } else { target };
        self.buf.fill_addr(at, addr);
    }

    /// The shift of the bytes at `at` under compression.
    fn delta_at(&self, at: Addr, delta: &[usize]) -> usize {
        match self.offsets.partition_point(|&o| o < at.0) {
            0 => 0,
            n => delta[n - 1],
        }
    }

    fn adjusted(&self, x: Addr, delta: &[usize]) -> Addr {
        Addr(x.0 - self.delta_at(x, delta))
    }

    /// The adjusted absolute or relative address of reference `i`.
    fn adjusted_offset(&self, i: usize, delta: &[usize]) -> Addr {
        let at = Addr(self.offsets[i]);
        let original = self.buf.get_nat(at);
        if !self.is_relative[i] {
            return self.adjusted(Addr(original), delta);
        }
        let start = self.buf.skip_nat(at);
        let len1 = original + delta[i] - self.delta_at(Addr(original + start.0), delta);
        assert_eq!(
            len1,
            self.adjusted(Addr(original + start.0), delta).0 - self.adjusted(start, delta).0,
            "adjusting offset #{i} at {at:?}"
        );
        Addr(len1)
    }

    fn compute_deltas(&self) -> Vec<usize> {
        let mut last = 0;
        let deltas = self.offsets.iter().map(|&off| {
            let skipped = self.buf.skip_zeroes(Addr(off)).0 - off;
            assert!(skipped < ADDR_WIDTH, "unset field at position {off}");
            last += skipped;
            last
        });
        deltas.collect()
    }

    /// Deltas that also count the zeroes of references shrunk under `delta`, with the bytes saved.
    fn adjust_deltas(&self, delta: &[usize]) -> (Vec<usize>, usize) {
        let mut last = 0;
        let delta1: Vec<usize> = (0..self.offsets.len())
            .map(|i| {
                last += ADDR_WIDTH - nat_size(self.adjusted_offset(i, delta).0);
                last
            })
            .collect();
        let saved = delta1.last().map_or(0, |&d1| d1 - delta[delta.len() - 1]);
        (delta1, saved)
    }

    fn compress(&mut self, delta: &[usize]) {
        let bytes = &mut self.buf.bytes;
        let mut last = 0;
        let mut start = 0;
        for (&next, &d) in self.offsets.iter().zip(delta) {
            bytes.copy_within(start..next, start - last);
            start = next + d - last;
            assert!(bytes[next..start].iter().all(|&b| b == 0), "eliminated non-zero");
            last = d;
        }
        let len = bytes.len();
        bytes.copy_within(start..len, start - last);
        bytes.truncate(len - last);
    }

    /// Shrinks every reference field to its smallest size and moves the bytes and tree addresses
    /// accordingly; stops refining when a round saves less than 1 %.
    pub fn compactify(&mut self) {
        let mut delta = self.compute_deltas();
        loop {
            let (delta1, saved) = self.adjust_deltas(&delta);
            delta = delta1;
            if saved == 0 || self.buf.len() / saved >= 100 {
                break;
            }
        }
        for i in 0..self.offsets.len() {
            let corrected = self.adjusted_offset(i, &delta);
            self.buf.fill_addr(Addr(self.offsets[i]), corrected);
        }
        let addrs: Vec<(u32, Addr)> =
            self.tree_addrs.iter().map(|(&id, &a)| (id, self.adjusted(a, &delta))).collect();
        self.tree_addrs.extend(addrs);
        self.compress(&delta);
    }
}
