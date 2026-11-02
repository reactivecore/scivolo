// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/PositionPickler.scala
//! The Positions section: the line sizes of the source, then the spans of trees by address,
//! each as deltas to the one before (dotty's `pickleLinesSizes`, `pickleDeltas`, `pickleSource`).

use super::buffer::{Addr, Buffer};
use crate::names::NameRef;
use crate::positions::{SOURCE, TastySpan};
use rustc_hash::FxHashSet;

#[derive(Debug, Default)]
pub struct PositionsBuffer {
    pub buf: Buffer,
    last_index: i64,
    last_start: i64,
    last_end: i64,
    pickled: FxHashSet<usize>,
}

/// The size of each line of `text`, in UTF-16 units (dotty's `pickleLinesSizes`).
pub fn text_line_sizes(text: &str) -> Vec<u32> {
    text.split('\n').map(|line| line.encode_utf16().count() as u32).collect()
}

impl PositionsBuffer {
    /// A section for a source of lines `line_sizes`.
    pub fn new(line_sizes: &[u32]) -> PositionsBuffer {
        let mut p = PositionsBuffer::default();
        p.buf.write_nat(line_sizes.len() as u64);
        line_sizes.iter().for_each(|&n| p.buf.write_nat(n as u64));
        p
    }

    /// Whether the span of the tree at `addr` is written.
    pub fn is_pickled(&self, addr: Addr) -> bool {
        self.pickled.contains(&addr.0)
    }

    /// Span `span` of the tree at `addr`.
    pub fn span(&mut self, addr: Addr, span: TastySpan) {
        let index = addr.0 as i64;
        let (start, end) = (span.start as i64 - self.last_start, span.end as i64 - self.last_end);
        let header = (index - self.last_index) << 3
            | i64::from(start != 0) << 2
            | i64::from(end != 0) << 1
            | i64::from(span.point.is_some());
        self.buf.write_long_int(header);
        for delta in [start, end].into_iter().filter(|&d| d != 0) {
            self.buf.write_long_int(delta);
        }
        if let Some(point) = span.point {
            self.buf.write_long_int(point as i64 - span.start as i64);
        }
        (self.last_index, self.last_start, self.last_end) = (index, span.start as i64, span.end as i64);
        self.pickled.insert(addr.0);
    }

    /// A change to the source of path `path`.
    pub fn source(&mut self, path: NameRef) {
        self.buf.write_long_int(SOURCE);
        self.buf.write_long_int(path.0 as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::{PositionsBuffer, text_line_sizes};
    use crate::positions::{TastySpan, decode_positions};
    use crate::write::Addr;

    #[test]
    fn spans_written_read_back() {
        let mut p = PositionsBuffer::new(&text_line_sizes("ab\n\ncdé"));
        p.span(Addr(2), TastySpan { start: 10, end: 20, point: Some(13) });
        p.source(crate::names::NameRef(5));
        p.span(Addr(3), TastySpan { start: 10, end: 25, point: None });
        assert_eq!(crate::positions::line_sizes(&p.buf.bytes).unwrap(), [2, 0, 3]);
        let spans = decode_positions(&p.buf.bytes).unwrap();
        assert_eq!(spans[&2], TastySpan { start: 10, end: 20, point: Some(13) });
        assert_eq!(spans[&3], TastySpan { start: 10, end: 25, point: None });
        assert!(p.is_pickled(Addr(3)) && !p.is_pickled(Addr(4)));
    }
}
