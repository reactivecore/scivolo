//! Typed trees: dotty's `tpd` trees in an arena per unit, each node with its type and, for
//! references and definitions, its symbol.

mod children;
mod info;
pub mod print;
mod tree;
mod walk;

pub use children::{MapChild, map_children};
pub use tree::{Clauses, Field, List, TNodeId, Tree};
pub use walk::{Leave, Visit, Walk, traverse, traverse_up};

use syntax::ast::Span;
use tasty::{SymbolId, TypeId};

pub struct Node {
    pub tree: Tree,
    pub span: Span,
    /// `None` for trees without a type: `Template`, `CaseDef`, `Import`.
    pub tpe: Option<TypeId>,
    pub sym: Option<SymbolId>,
}

#[derive(Default)]
pub struct Tpd {
    nodes: Vec<Node>,
    lists: Vec<TNodeId>,
    clauses: Vec<List>,
}

impl Tpd {
    pub fn node(&mut self, tree: Tree, span: Span, tpe: Option<TypeId>, sym: Option<SymbolId>) -> TNodeId {
        self.nodes.push(Node { tree, span, tpe, sym });
        TNodeId::new(self.nodes.len() - 1)
    }

    /// How many trees the arena holds.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, id: TNodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn get_mut(&mut self, id: TNodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    pub fn tree(&self, id: TNodeId) -> &Tree {
        &self.get(id).tree
    }

    /// The type of `id`, which must be a term or type tree.
    pub fn tpe(&self, id: TNodeId) -> TypeId {
        self.get(id).tpe.expect("a typed tree")
    }

    pub fn sym(&self, id: TNodeId) -> Option<SymbolId> {
        self.get(id).sym
    }

    pub fn span(&self, id: TNodeId) -> Span {
        self.get(id).span
    }

    pub fn list(&mut self, items: &[TNodeId]) -> List {
        let start = self.lists.len();
        self.lists.extend_from_slice(items);
        List::new(start, items.len())
    }

    pub fn items(&self, l: List) -> &[TNodeId] {
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
}
