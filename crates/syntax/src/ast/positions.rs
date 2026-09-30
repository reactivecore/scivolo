//! Positions as dotty's `Positioned` assigns them.

use super::children::Child;
use super::span::Span;
use super::tree::{ModsId, NodeId, Tree};
use super::{Ast, Attrs, Node};

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

    /// `id` positioned at `span` as dotty's `withSpan` returns it: a node without a span gets
    /// it (and its children theirs), a node with another one is copied first.
    pub fn respan(&mut self, id: NodeId, span: Span) -> NodeId {
        match *self.tree(id) {
            Tree::Empty => id,
            Tree::Thicket { trees } => {
                let items = self.items(trees).to_vec();
                let respanned: Vec<NodeId> = items.iter().map(|&t| self.respan(t, span)).collect();
                if respanned == items {
                    return id;
                }
                let trees = self.list(&respanned);
                self.node(Tree::Thicket { trees })
            }
            _ => {
                let old = self.get(id).span;
                if span == old {
                    return id;
                }
                let id = if old.exists() {
                    self.copy_node(id)
                } else {
                    if span.exists() {
                        self.envelope(id, span.start_pos());
                    }
                    id
                };
                self.get_mut(id).span = span;
                id
            }
        }
    }

    /// `tree` as a copy of `orig` with other fields (dotty's `cpy`): `orig` itself if `tree` is
    /// the same, else a new node with the span and attributes of `orig`. A copied XML block is
    /// a block, a copied placeholder function a function.
    pub fn derived(&mut self, orig: NodeId, mut tree: Tree) -> NodeId {
        if *self.tree(orig) == tree {
            return orig;
        }
        if let Tree::Function { wildcard, .. } = &mut tree {
            *wildcard = false;
        }
        let (span, attrs) = (self.get(orig).span, self.get(orig).attrs);
        let id = self.node(tree);
        if !self.get(id).span.exists() && span.exists() {
            self.envelope(id, span.start_pos());
        }
        let node = self.get_mut(id);
        node.span = span;
        node.attrs = node.attrs.union(attrs);
        self.remove_attrs(id, Attrs::XML_BLOCK);
        id
    }

    /// A new node with the tree, span and attributes of `id`.
    pub fn copy_node(&mut self, id: NodeId) -> NodeId {
        let node = self.get(id);
        let copy = Node { tree: node.tree.clone(), span: node.span, attrs: node.attrs };
        self.nodes.push(copy);
        NodeId::new(self.nodes.len() - 1)
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
