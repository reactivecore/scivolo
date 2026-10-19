// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/util/Spans.scala
//! Spans as dotty's `util.Spans`: start, end, and a point unless the span is synthetic.

use crate::source::Offset;

/// The point delta marking a synthetic span.
const SYNTHETIC: u16 = 4095;

/// The offset bit marking a span in another source file.
const OTHER_SOURCE: Offset = 1 << 31;

/// The offset bit marking the span of an inline call given to trees from another source.
const INLINED_CALL: Offset = 1 << 30;

/// The length a source may not reach, below the offset bits of spans.
pub const MAX_SOURCE_LEN: usize = INLINED_CALL as usize;

/// `point - start` in the 12 bits of dotty's point delta; 4095 reads as synthetic.
fn wrapped_delta(start: Offset, point: Offset) -> u16 {
    (point.wrapping_sub(start) & SYNTHETIC as u32) as u16
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    start: Offset,
    end: Offset,
    point_delta: u16,
}

impl Span {
    /// The span that does not exist.
    pub const NONE: Span = Span { start: 1, end: 0, point_delta: SYNTHETIC };

    /// A synthetic span.
    pub fn synthetic(start: Offset, end: Offset) -> Span {
        Span { start, end, point_delta: SYNTHETIC }
    }

    /// A synthetic zero-extent span.
    pub fn at(offset: Offset) -> Span {
        Span::synthetic(offset, offset)
    }

    /// A source-derived span; a point too far from the start becomes the start.
    pub fn new(start: Offset, end: Offset, point: Offset) -> Span {
        let delta = point.saturating_sub(start);
        Span { start, end, point_delta: if delta >= SYNTHETIC as u32 { 0 } else { delta as u16 } }
    }

    pub fn exists(self) -> bool {
        self != Span::NONE
    }

    pub fn start(self) -> Offset {
        self.start
    }

    pub fn end(self) -> Offset {
        self.end
    }

    /// The point, the start for a synthetic span.
    pub fn point(self) -> Offset {
        if self.is_synthetic() { self.start } else { self.start + self.point_delta as u32 }
    }

    pub fn is_synthetic(self) -> bool {
        self.point_delta == SYNTHETIC
    }

    pub fn is_source_derived(self) -> bool {
        !self.is_synthetic()
    }

    /// The least span enclosing both, with `self`'s point; a span of an inlined call, and a span
    /// with one, stays as it is.
    pub fn union(self, that: Span) -> Span {
        if !self.exists() {
            that
        } else if !that.exists() || self.is_inlined_call() || that.is_inlined_call() {
            self
        } else {
            Span::new(self.start.min(that.start), self.end.max(that.end), self.point())
        }
    }

    pub fn start_pos(self) -> Span {
        if self.exists() && !self.is_inlined_call() { Span::at(self.start) } else { self }
    }

    pub fn end_pos(self) -> Span {
        if self.exists() && !self.is_inlined_call() { Span::at(self.end) } else { self }
    }

    pub fn with_start(self, start: Offset) -> Span {
        if !self.exists() || self.is_inlined_call() {
            return self;
        }
        let delta = if self.is_synthetic() { SYNTHETIC } else { wrapped_delta(start, self.point()) };
        Span { start, end: self.end, point_delta: delta }
    }

    pub fn with_end(self, end: Offset) -> Span {
        if self.exists() && !self.is_inlined_call() { Span { end, ..self } } else { self }
    }

    pub fn with_point(self, point: Offset) -> Span {
        match self.exists() && !self.is_inlined_call() {
            true => Span { point_delta: wrapped_delta(self.start, point), ..self },
            false => self,
        }
    }

    pub fn to_synthetic(self) -> Span {
        if self.is_synthetic() || self.is_inlined_call() {
            self
        } else {
            Span::synthetic(self.start, self.end)
        }
    }

    /// This span of an inline call, as trees inlined from another source take it: the spans
    /// derived from it are it.
    pub fn as_inlined_call(self) -> Span {
        if !self.exists() {
            return self;
        }
        Span { start: self.start | INLINED_CALL, end: self.end | INLINED_CALL, ..self }
    }

    pub fn is_inlined_call(self) -> bool {
        self.exists() && self.start & INLINED_CALL != 0
    }

    /// This span without the mark of `as_inlined_call`.
    pub fn without_inlined_call(self) -> Span {
        if !self.is_inlined_call() {
            return self;
        }
        Span { start: self.start & !INLINED_CALL, end: self.end & !INLINED_CALL, ..self }
    }

    /// The span at the same offsets of another source file, a library's.
    pub fn in_other_source(self) -> Span {
        if !self.exists() || self.is_in_other_source() {
            return self;
        }
        Span { start: self.start | OTHER_SOURCE, end: self.end | OTHER_SOURCE, ..self }
    }

    pub fn is_in_other_source(self) -> bool {
        self.exists() && self.start & OTHER_SOURCE != 0
    }

    /// The span with its offsets in its own source file, wherever that is.
    pub fn in_own_source(self) -> Span {
        if !self.is_in_other_source() {
            return self;
        }
        Span { start: self.start & !OTHER_SOURCE, end: self.end & !OTHER_SOURCE, ..self }
    }

    /// This span `by` bytes later, as in a source placed after others.
    pub fn shifted(self, by: Offset) -> Span {
        if !self.exists() {
            return self;
        }
        Span { start: self.start + by, end: self.end + by, ..self }
    }

    pub fn contains(self, that: Span) -> bool {
        !that.exists() || self.exists() && self.start <= that.start && self.end >= that.end
    }
}
