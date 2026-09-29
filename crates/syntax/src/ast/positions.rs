//! Positions as dotty's `Positioned` assigns them.

use super::Ast;
use super::children::Child;
use super::span::Span;
use super::tree::{ModsId, NodeId, Tree};

/// The largest offset a span can hold, used as a marker while computing envelopes.
const MAX_OFFSET: u32 = (1 << 26) - 1;

impl Ast {
    /// Give `id` the span `span`, first filling in the spans of children without one if the
    /// node had none (dotty's `withSpan`).
    pub fn with_span(&mut self, id: NodeId, span: Span) {
        if matches!(self.tree(id), Tree::Empty) {
            return;
        }
        if let Tree::Thicket { trees } = *self.tree(id) {
            for i in 0..trees.len() {
                let t = self.items(trees)[i];
                self.with_span(t, span);
            }
            return;
        }
        let old = self.span(id);
        if span == old {
            return;
        }
        if !old.exists() && span.exists() {
            self.envelope(id, span.start_pos());
        }
        self.get_mut(id).span = span;
    }

    /// Position `id` at `span` unless it already has a source-derived span (dotty's `atSpan`).
    pub fn at_span(&mut self, id: NodeId, span: Span) -> NodeId {
        let old = self.span(id);
        if old.is_source_derived() || matches!(self.tree(id), Tree::Empty) {
            return id;
        }
        self.with_span(id, span.union(old));
        id
    }

    /// The union of `start` and the children's spans; children without a span get one at the
    /// end of what precedes them (dotty's `envelope`).
    pub fn envelope(&mut self, id: NodeId, start: Span) -> Span {
        let children = self.children(id);
        let span1 = children.iter().fold(start, |s, &c| self.include(s, c));
        let span2 = if !span1.exists() || span1.end() != MAX_OFFSET {
            span1
        } else if span1.start() == MAX_OFFSET {
            Span::NONE
        } else {
            children.iter().fold(span1.start_pos(), |s, &c| self.include(s, c))
        };
        span2.to_synthetic()
    }

    fn include(&mut self, span: Span, child: Child) -> Span {
        match child {
            Child::Empty => span,
            Child::Node(p) => self.include_positioned(span, PosRef::Node(p)),
            Child::List(l) => {
                (0..l.len()).fold(span, |s, i| self.include_positioned(s, PosRef::Node(self.items(l)[i])))
            }
            Child::Mods(m) => {
                let n = self.modifiers(m).mods.len();
                let s = (0..n).fold(span, |s, i| self.include_positioned(s, PosRef::Mod(m, i)));
                let annots = self.modifiers(m).annotations;
                self.include(s, Child::List(annots))
            }
            Child::Clauses(c) => {
                (0..c.len()).fold(span, |s, i| self.include(s, Child::List(self.clause_items(c)[i])))
            }
        }
    }

    fn include_positioned(&mut self, span: Span, p: PosRef) -> Span {
        if let PosRef::Node(n) = p
            && matches!(self.tree(n), Tree::Empty)
        {
            return span;
        }
        let own = match p {
            PosRef::Node(n) => self.span(n),
            PosRef::Mod(m, i) => self.modifiers(m).mods[i].span,
        };
        if own.exists() {
            span.union(own)
        } else if span.exists() {
            if span.end() != MAX_OFFSET {
                match p {
                    PosRef::Node(n) => {
                        let s = self.envelope(n, span.end_pos());
                        self.get_mut(n).span = s;
                    }
                    PosRef::Mod(m, i) => self.modifiers_mut(m).mods[i].span = span.end_pos(),
                }
            }
            span
        } else {
            Span::synthetic(MAX_OFFSET, MAX_OFFSET)
        }
    }
}

#[derive(Clone, Copy)]
enum PosRef {
    Node(NodeId),
    Mod(ModsId, usize),
}
