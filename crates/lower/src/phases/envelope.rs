//! The spans of trees made by the phases, as dotty's `Positioned` gives them: the union of
//! their children's, and to children without one the end of those before, else the start of
//! those after.

use super::walk::children;
use crate::Lower;
use syntax::ast::Span;
use syntax::source::Offset;
use typer::tpd::TNodeId;

/// dotty's `MaxOffset`, the end of a span marking children whose spans are still to assign.
const MAX_OFFSET: Offset = (1 << 26) - 1;

impl Lower<'_> {
    /// Gives `t` the span of its children if it has none; `t`.
    pub fn positioned(&mut self, t: TNodeId) -> TNodeId {
        if !self.tpd.span(t).exists() {
            let span = self.envelope(t, Span::NONE);
            self.tpd.get_mut(t).span = span;
        }
        t
    }

    /// Gives `t` span `span`, first assigning its children spans from `span`'s start if `t` has
    /// none (dotty's `withSpan`); `t`.
    pub fn with_span(&mut self, t: TNodeId, span: Span) -> TNodeId {
        if !self.tpd.span(t).exists() && span.exists() {
            self.envelope(t, span.start_pos());
        }
        self.tpd.get_mut(t).span = span;
        t
    }

    /// Copies `t` as dotty's tree copiers do after a child changed: its span recomputed from its
    /// children, assigning spans to those without, then set back with `with_span`.
    pub fn recopy(&mut self, t: TNodeId) {
        let span = self.tpd.span(t);
        self.tpd.get_mut(t).span = Span::NONE;
        self.positioned(t);
        self.with_span(t, span);
    }

    /// Assigns spans to those of `trees` without, as a dotty `Thicket` of them does.
    pub fn position_thicket(&mut self, trees: &[TNodeId]) {
        self.envelope_of(trees, Span::NONE);
    }

    /// The union of `start` and the spans of `t`'s children, assigning spans to those without.
    fn envelope(&mut self, t: TNodeId, start: Span) -> Span {
        let parts = children(self.tpd, t);
        self.envelope_of(&parts, start)
    }

    fn envelope_of(&mut self, parts: &[TNodeId], start: Span) -> Span {
        let span = self.include_all(start, parts);
        let span = if !span.exists() || span.end() != MAX_OFFSET {
            span
        } else if span.start() == MAX_OFFSET {
            Span::NONE
        } else {
            self.include_all(span.start_pos(), parts)
        };
        span.to_synthetic()
    }

    fn include_all(&mut self, start: Span, parts: &[TNodeId]) -> Span {
        parts.iter().fold(start, |span, &p| self.include(span, p))
    }

    fn include(&mut self, span: Span, p: TNodeId) -> Span {
        let own = self.tpd.span(p);
        if own.exists() {
            span.union(own)
        } else if span.exists() {
            if span.end() != MAX_OFFSET {
                let assigned = self.envelope(p, span.end_pos());
                self.tpd.get_mut(p).span = assigned;
            }
            span
        } else {
            Span::synthetic(MAX_OFFSET, MAX_OFFSET)
        }
    }
}
