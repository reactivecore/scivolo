//! Untyped trees, as dotty's parser produces them, and the `Ast` that holds the trees of one
//! source file.

pub mod children;
pub mod constant;
mod copy;
pub mod flags;
mod kinds;
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
pub struct Attrs(u32);

impl Attrs {
    pub const BACKQUOTED: Attrs = Attrs(1);
    pub const HAS_END_MARKER: Attrs = Attrs(1 << 1);
    /// An `Apply` of a `using` argument clause.
    pub const USING_APPLY: Attrs = Attrs(1 << 2);
    /// A `()` literal the parser or desugaring inserted.
    pub const SYNTHETIC_UNIT: Attrs = Attrs(1 << 3);
    /// A capture variable `C^`, on its definition and its bounds.
    pub const CAPTURE_VAR: Attrs = Attrs(1 << 4);
    /// A `@retains` annotation made from a capture set.
    pub const RETAINS_ANNOT: Attrs = Attrs(1 << 5);
    /// A `Block` an XML literal made (dotty's `XMLBlock`).
    pub const XML_BLOCK: Attrs = Attrs(1 << 6);
    /// An `Apply` of an infix operator to a tuple, `a op (b, c)`.
    pub const INFIX_TUPLE_APPLY: Attrs = Attrs(1 << 7);
    /// A tuple of pattern variables that desugaring made.
    pub const FOR_ARTIFACT: Attrs = Attrs(1 << 8);
    /// A definition of a pattern variable.
    pub const PATTERN_VAR: Attrs = Attrs(1 << 9);
    /// The `map` of a `for` whose body is its pattern.
    pub const TRAILING_FOR_MAP: Attrs = Attrs(1 << 10);
    /// A `map` of a `for` over several aliases.
    pub const TUPLING_FOR_MAP: Attrs = Attrs(1 << 11);
    /// An operator selection whose operator starts a new line.
    pub const MULTI_LINE_INFIX: Attrs = Attrs(1 << 12);
    /// An `Apply` made of an infix operation.
    pub const WAS_TYPED_INFIX: Attrs = Attrs(1 << 13);
    /// A selector checked to match the pattern of a pattern definition.
    pub const CHECK_PAT_DEF: Attrs = Attrs(1 << 14);
    /// A selector checked to match the pattern of a generator.
    pub const CHECK_GEN_FROM: Attrs = Attrs(1 << 15);
    /// An evidence parameter made of a context bound.
    pub const EVIDENCE: Attrs = Attrs(1 << 16);
    /// A companion object that receives the `derives` clause of its class.
    pub const DERIVING_COMPANION: Attrs = Attrs(1 << 17);

    pub fn contains(self, a: Attrs) -> bool {
        self.0 & a.0 == a.0
    }

    pub fn union(self, a: Attrs) -> Attrs {
        Attrs(self.0 | a.0)
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
    /// An `Ast` with room for the trees of `source_len` bytes of source, up to 1 MiB: one tree
    /// per 5 bytes, as in dotty's test suites.
    pub fn for_source(source_len: usize) -> Ast {
        let source_len = source_len.min(1 << 20);
        Ast {
            nodes: Vec::with_capacity(source_len / 5),
            lists: Vec::with_capacity(source_len / 12),
            clauses: Vec::with_capacity(source_len / 128),
            mods: Vec::with_capacity(source_len / 20),
        }
    }

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

    pub fn remove_attrs(&mut self, id: NodeId, a: Attrs) {
        self.get_mut(id).attrs.0 &= !a.0;
    }

    pub fn attrs(&self, id: NodeId) -> Attrs {
        self.get(id).attrs
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
