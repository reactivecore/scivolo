//! Spans as dotty's `util.Spans`: start, end, and a point unless the span is synthetic.

use crate::source::Offset;

/// The point delta marking a synthetic span.
const SYNTHETIC: u16 = 4095;

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

    /// The least span enclosing both, with `self`'s point.
    pub fn union(self, that: Span) -> Span {
        if !self.exists() {
            that
        } else if !that.exists() {
            self
        } else {
            Span::new(self.start.min(that.start), self.end.max(that.end), self.point())
        }
    }

    pub fn start_pos(self) -> Span {
        if self.exists() { Span::at(self.start) } else { self }
    }

    pub fn end_pos(self) -> Span {
        if self.exists() { Span::at(self.end) } else { self }
    }

    pub fn with_start(self, start: Offset) -> Span {
        if !self.exists() {
            return self;
        }
        let delta = if self.is_synthetic() { SYNTHETIC } else { wrapped_delta(start, self.point()) };
        Span { start, end: self.end, point_delta: delta }
    }

    pub fn with_end(self, end: Offset) -> Span {
        if self.exists() { Span { end, ..self } } else { self }
    }

    pub fn with_point(self, point: Offset) -> Span {
        if self.exists() { Span { point_delta: wrapped_delta(self.start, point), ..self } } else { self }
    }

    pub fn to_synthetic(self) -> Span {
        if self.is_synthetic() { self } else { Span::synthetic(self.start, self.end) }
    }

    pub fn contains(self, that: Span) -> bool {
        !that.exists() || self.exists() && self.start <= that.start && self.end >= that.end
    }
}
