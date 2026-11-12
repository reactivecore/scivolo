//! Typed trees: dotty's `tpd` trees in an arena per unit, each node with its type and, for
//! references and definitions, its symbol.

mod build;
mod children;
mod info;
pub mod print;
mod slots;
mod tree;
mod walk;

pub use build::{Build, constant_of, constant_type_of};
pub use children::{Children, MapChild, map_children};
pub use info::{Template, lub_of};
pub use slots::Slot;
pub use tree::{Clauses, Field, List, MatchKind, TNodeId, Tree};
pub use walk::{Leave, Visit, Walk, traverse, traverse_up};

use rustc_hash::{FxHashMap, FxHashSet};
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
    trailing_maps: FxHashSet<TNodeId>,
    backquoted: FxHashSet<TNodeId>,
    anon_mirrors: FxHashMap<SymbolId, AnonMirror>,
    quote_tags: FxHashMap<TNodeId, Vec<(SymbolId, TypeId)>>,
    pattern_givens: FxHashMap<SymbolId, SymbolId>,
    annotations: FxHashMap<SymbolId, Vec<TNodeId>>,
    copies: FxHashMap<TNodeId, TNodeId>,
    read_from: FxHashMap<TNodeId, SymbolId>,
    /// The annotation tree of each annotated type an annotated expression `e: @a(args)` gave;
    /// `None` for one that several trees gave.
    type_annotations: FxHashMap<TypeId, Option<TNodeId>>,
    /// Whether the unit behaves as the Scala 2 library (dotty's `shouldBehaveAsScala2`).
    pub scala2: bool,
}

/// What an anonymous class the typer made for a mirror becomes (dotty's
/// `ExtendsSumOrProductMirror` attachment).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AnonMirror {
    Product,
    Sum,
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

    /// Marks `id`, the `map` call a `for` ends in, as one returning its generator's pattern or
    /// `()` (dotty's `TrailingForMap`).
    pub fn mark_trailing_map(&mut self, id: TNodeId) {
        self.trailing_maps.insert(id);
    }

    pub fn unmark_trailing_map(&mut self, id: TNodeId) {
        self.trailing_maps.remove(&id);
    }

    pub fn is_trailing_map(&self, id: TNodeId) -> bool {
        self.trailing_maps.contains(&id)
    }

    /// Marks `id`, a pattern identifier written in back-quotes (dotty's `Backquoted`).
    pub fn mark_backquoted(&mut self, id: TNodeId) {
        self.backquoted.insert(id);
    }

    pub fn is_backquoted(&self, id: TNodeId) -> bool {
        self.backquoted.contains(&id)
    }

    /// Marks anonymous class `class` as a mirror of kind `kind`.
    pub fn mark_anon_mirror(&mut self, class: SymbolId, kind: AnonMirror) {
        self.anon_mirrors.insert(class, kind);
    }

    pub fn anon_mirror(&self, class: SymbolId) -> Option<AnonMirror> {
        self.anon_mirrors.get(&class).copied()
    }

    /// Records for quote `quote` the given `Type[T]` of each type `T` of an outer level it refers to.
    pub fn set_quote_tags(&mut self, quote: TNodeId, tags: Vec<(SymbolId, TypeId)>) {
        self.quote_tags.insert(quote, tags);
    }

    /// The types of an outer level quote `quote` refers to, each with the path of its `Type[T]`.
    pub fn quote_tags(&self, quote: TNodeId) -> &[(SymbolId, TypeId)] {
        self.quote_tags.get(&quote).map_or(&[], Vec::as_slice)
    }

    /// Records `given`, the given `Type[t]` the encoded quote pattern binds for its type variable `t`.
    pub fn set_pattern_given(&mut self, t: SymbolId, given: SymbolId) {
        self.pattern_givens.insert(t, given);
    }

    /// The given `Type[t]` of quote pattern type variable `t`.
    pub fn pattern_given(&self, t: SymbolId) -> Option<SymbolId> {
        self.pattern_givens.get(&t).copied()
    }

    /// Records annotation tree `annot` of definition `sym`, which a phase made.
    pub fn annotate(&mut self, sym: SymbolId, annot: TNodeId) {
        self.annotations.entry(sym).or_default().push(annot);
    }

    /// Records `t` as a tree read from the TASTy of library definition `sym`.
    pub fn mark_read_from(&mut self, t: TNodeId, sym: SymbolId) {
        self.read_from.insert(t, sym);
    }

    /// The library definition from whose TASTy `t` was read, if marked.
    pub fn read_from(&self, t: TNodeId) -> Option<SymbolId> {
        self.read_from.get(&t).copied()
    }

    /// The annotation trees phases recorded for definition `sym`.
    pub fn annotations(&self, sym: SymbolId) -> &[TNodeId] {
        self.annotations.get(&sym).map_or(&[], Vec::as_slice)
    }

    /// Records `annot` as the annotation tree of annotated type `annotated`.
    pub fn set_type_annotation(&mut self, annotated: TypeId, annot: TNodeId) {
        let tree = self.type_annotations.entry(annotated).or_insert(Some(annot));
        if *tree != Some(annot) {
            *tree = None;
        }
    }

    /// The annotation tree of annotated type `annotated`, if one annotated expression gave it.
    pub fn type_annotation(&self, annotated: TypeId) -> Option<TNodeId> {
        self.type_annotations.get(&annotated).copied().flatten()
    }

    /// Marks `copy` as a copy of `of` that is one tree with it in dotty's.
    pub fn mark_copy(&mut self, copy: TNodeId, of: TNodeId) {
        self.copies.insert(copy, self.copy_of(of).unwrap_or(of));
    }

    /// The tree `id` is a copy of, if marked one.
    pub fn copy_of(&self, id: TNodeId) -> Option<TNodeId> {
        self.copies.get(&id).copied()
    }
}
