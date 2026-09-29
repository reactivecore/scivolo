//! Untyped trees, as dotty's parser produces them, and the `Ast` that holds the trees of one
//! source file.

pub mod children;
pub mod constant;
pub mod flags;
pub mod modifiers;
mod positions;
pub mod print;
pub mod span;
pub mod tree;

pub use span::Span;
pub use tree::{List, NodeId, TName, Tree};

use modifiers::Modifiers;
use tree::{Clauses, ModsId};

/// Facts dotty keeps as tree attachments.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Attrs(u8);

impl Attrs {
    pub const BACKQUOTED: Attrs = Attrs(1);
    pub const HAS_END_MARKER: Attrs = Attrs(1 << 1);
    /// An `Apply` of a `using` argument clause.
    pub const USING_APPLY: Attrs = Attrs(1 << 2);
    /// A `()` literal the parser inserted.
    pub const SYNTHETIC_UNIT: Attrs = Attrs(1 << 3);
    /// A capture variable `C^`, on its definition and its bounds.
    pub const CAPTURE_VAR: Attrs = Attrs(1 << 4);
    /// A `@retains` annotation made from a capture set.
    pub const RETAINS_ANNOT: Attrs = Attrs(1 << 5);

    pub fn contains(self, a: Attrs) -> bool {
        self.0 & a.0 == a.0
    }
}

pub struct Node {
    pub tree: Tree,
    pub span: Span,
    pub attrs: Attrs,
}

/// The trees of one source file with their spans, the lists of trees they hold, and their
/// modifiers, addressed by `NodeId`, `List`, `Clauses` and `ModsId`.
#[derive(Default)]
pub struct Ast {
    nodes: Vec<Node>,
    lists: Vec<NodeId>,
    clauses: Vec<List>,
    mods: Vec<Modifiers>,
}

impl Ast {
    /// A new node, spanning the envelope of its children as in dotty.
    pub fn node(&mut self, tree: Tree) -> NodeId {
        self.nodes.push(Node { tree, span: Span::NONE, attrs: Attrs::default() });
        let id = NodeId::new(self.nodes.len() - 1);
        let span = self.envelope(id, Span::NONE);
        self.get_mut(id).span = span;
        id
    }

    pub fn get(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn get_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    pub fn tree(&self, id: NodeId) -> &Tree {
        &self.get(id).tree
    }

    /// The span of `id`; a `Thicket`'s is the union of its trees' spans.
    pub fn span(&self, id: NodeId) -> Span {
        match self.tree(id) {
            Tree::Thicket { trees } => {
                self.items(*trees).iter().fold(Span::NONE, |s, &t| s.union(self.span(t)))
            }
            _ => self.get(id).span,
        }
    }

    pub fn add_attrs(&mut self, id: NodeId, a: Attrs) {
        self.get_mut(id).attrs.0 |= a.0;
    }

    pub fn list(&mut self, items: &[NodeId]) -> List {
        let start = self.lists.len();
        self.lists.extend_from_slice(items);
        List::new(start, items.len())
    }

    pub fn items(&self, l: List) -> &[NodeId] {
        &self.lists[l.range()]
    }

    pub fn clauses(&mut self, items: &[List]) -> Clauses {
        let start = self.clauses.len();
        self.clauses.extend_from_slice(items);
        Clauses::new(start, items.len())
    }

    pub fn clause_items(&self, c: Clauses) -> &[List] {
        &self.clauses[c.range()]
    }

    pub fn mods(&mut self, m: Modifiers) -> ModsId {
        self.mods.push(m);
        ModsId::new(self.mods.len() - 1)
    }

    pub fn modifiers(&self, id: ModsId) -> &Modifiers {
        &self.mods[id.index()]
    }

    pub fn modifiers_mut(&mut self, id: ModsId) -> &mut Modifiers {
        &mut self.mods[id.index()]
    }
}
