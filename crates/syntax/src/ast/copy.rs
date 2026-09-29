//! Copies of trees as dotty's `cpy`, `withSpan` and `withMods` make them.

use super::modifiers::Modifiers;
use super::span::Span;
use super::tree::{ModsId, NodeId, Tree};
use super::{Ast, Attrs, Node};

impl Ast {
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

    /// The modifiers of definition `id`.
    pub fn mods_of(&self, id: NodeId) -> Option<ModsId> {
        match *self.tree(id) {
            Tree::ValDef { mods, .. }
            | Tree::DefDef { mods, .. }
            | Tree::TypeDef { mods, .. }
            | Tree::ModuleDef { mods, .. }
            | Tree::Bind { mods, .. }
            | Tree::PatDef { mods, .. } => Some(mods),
            _ => None,
        }
    }

    /// Definition `id` with modifiers `m` (dotty's `withMods`): `id` itself if they are its
    /// modifiers, else a copy.
    pub fn with_mods(&mut self, id: NodeId, m: Modifiers) -> NodeId {
        let Some(old) = self.mods_of(id) else { return id };
        if *self.modifiers(old) == m {
            return id;
        }
        let new = self.mods(m);
        let id = self.copy_node(id);
        match &mut self.get_mut(id).tree {
            Tree::ValDef { mods, .. }
            | Tree::DefDef { mods, .. }
            | Tree::TypeDef { mods, .. }
            | Tree::ModuleDef { mods, .. }
            | Tree::Bind { mods, .. }
            | Tree::PatDef { mods, .. } => *mods = new,
            _ => {}
        }
        id
    }
}
