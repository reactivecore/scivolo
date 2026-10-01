//! Typing untyped trees into `tpd` trees (dotty's `Typer`): the dispatch over tree classes.

mod access;
mod apply;
mod classes;
mod defs;
mod derived;
mod find_ref;
mod idents;
mod imports;
mod literals;
mod no_inits;
mod overrides;
mod parents;
mod patterns;
mod select;
mod stats;
mod this;
mod type_defs;
mod type_trees;

use crate::ctx::{Ctx, LevelId, Levels};
use crate::defn::Defn;
use crate::symbols::Syms;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::{Ast, NodeId, Span, TName, Tree};
use syntax::diag::Diagnostics;
use syntax::names::{Name, Names};
use tasty::{Type, TypeId, Universe};

/// What the context expects of a tree (dotty's prototypes).
#[derive(Clone, Copy, Debug)]
pub enum Pt {
    Any,
    Type(TypeId),
    /// A function applied to this many arguments (dotty's `FunProto`).
    Args(usize),
}

pub struct Typer<'a> {
    pub u: &'a mut Universe,
    pub ast: &'a mut Ast,
    pub names: &'a mut Names,
    pub defn: Defn,
    pub tpd: Tpd,
    pub levels: Levels,
    pub syms: Syms,
    pub diags: Diagnostics,
    /// Trees typed while completing a symbol, taken when typing reaches them.
    pub ahead: FxHashMap<NodeId, TNodeId>,
    /// Untyped trees that stand for typed ones (dotty's `TypedSplice`).
    pub splices: FxHashMap<NodeId, TNodeId>,
    /// The imports of the unit, root imports first, by `Level::Import` index.
    pub imports: Vec<imports::ImportEntry>,
    /// The level each import tree opens.
    pub import_levels: FxHashMap<NodeId, LevelId>,
    /// The first construct outside the subset, and where.
    pub unsupported: Option<(Span, String)>,
    /// How many `typed` calls are in progress, one inside another.
    depth: u32,
}

/// How many `typed` calls may be in progress at once.
const MAX_DEPTH: u32 = 400;

impl<'a> Typer<'a> {
    pub fn new(u: &'a mut Universe, ast: &'a mut Ast, names: &'a mut Names) -> Result<Self, String> {
        let defn = Defn::new(u)?;
        Ok(Typer {
            u,
            ast,
            names,
            defn,
            tpd: Tpd::default(),
            levels: Levels::default(),
            syms: Syms::default(),
            diags: Diagnostics::default(),
            ahead: FxHashMap::default(),
            splices: FxHashMap::default(),
            imports: Vec::new(),
            import_levels: FxHashMap::default(),
            unsupported: None,
            depth: 0,
        })
    }

    /// The tree typed for `id` ahead of typing it, or the typed tree it stands for.
    pub fn ready(&mut self, id: NodeId) -> Option<TNodeId> {
        if let Some(t) = self.ahead.remove(&id) {
            return Some(t);
        }
        let &t = self.splices.get(&id)?;
        self.tpd.get_mut(t).span = self.ast.span(id);
        Some(t)
    }

    pub fn typed(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if self.depth >= MAX_DEPTH {
            return self.unsupported(id, "nesting too deep");
        }
        self.depth += 1;
        let tree = match self.ready(id) {
            Some(t) => t,
            None => self.typed_unadapted(id, pt, ctx),
        };
        self.adapt(tree, pt);
        self.depth -= 1;
        tree
    }

    /// Checks that `tree` fits `pt` as it is: a method is applied, a value conforms; a tree that
    /// needs dotty's `adapt` to fit is outside the subset.
    fn adapt(&mut self, tree: TNodeId, pt: Pt) {
        let Some(tpe) = self.tpd.get(tree).tpe else { return };
        if matches!(self.u.types().get(tpe), Type::Error) || matches!(pt, Pt::Args(_)) {
            return;
        }
        let span = self.tpd.span(tree);
        let method = self.method_info(tpe).map(|i| self.u.types().get(i).clone());
        if matches!(method, Some(Type::Method { .. } | Type::Poly { .. })) {
            return self.unsupported_at(span, "method value");
        }
        if let Pt::Type(pt) = pt
            && !self.u.is_subtype(tpe, pt)
        {
            self.unsupported_at(span, "adaptation");
        }
    }

    fn typed_unadapted(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        match self.ast.tree(id).clone() {
            Tree::Ident { name } => self.typed_ident(id, name, pt, ctx),
            Tree::Select { qualifier, name } => self.typed_select(id, qualifier, name, pt, ctx),
            Tree::Apply { fun, args } => self.typed_apply(id, fun, args, ctx),
            Tree::Literal { value } => self.typed_literal(id, value),
            Tree::Number { digits, kind } => self.typed_number(id, &digits, kind, pt),
            Tree::TypedSplice { sym } => self.typed_full_ref(id, sym),
            Tree::This { qual } => self.typed_this(id, qual, ctx),
            Tree::New { tpt } => self.typed_new(id, tpt, ctx),
            Tree::Typed { expr, tpt } => self.typed_typed(id, expr, tpt, pt, ctx),
            Tree::Block { stats, expr } => self.typed_block(id, stats, expr, pt, ctx),
            Tree::Try { expr, cases, finalizer: None } => self.typed_try(id, expr, cases, pt, ctx),
            Tree::Bind { name, body, .. } => self.typed_bind(id, name, body, pt, ctx),
            Tree::TypeTree
            | Tree::AppliedTypeTree { .. }
            | Tree::SingletonTypeTree { .. }
            | Tree::TypeBoundsTree { .. }
            | Tree::DerivedTypeTree { .. } => self.typed_type(id, ctx),
            Tree::ValDef { .. } => self.typed_val_def(id, ctx),
            Tree::DefDef { .. } => self.typed_def_def(id, ctx),
            Tree::TypeDef { rhs, .. } if matches!(self.ast.tree(rhs), Tree::Template { .. }) => {
                self.typed_class_def(id, ctx)
            }
            Tree::TypeDef { .. } => self.typed_type_def(id, ctx),
            Tree::PackageDef { pid, stats } => self.typed_package_def(id, pid, stats, ctx),
            Tree::Import { selectors, .. } if self.import_levels.contains_key(&id) => {
                self.typed_import(id, selectors)
            }
            t => {
                let what = format!("{t:?}");
                self.unsupported(id, what.split([' ', '{']).next().unwrap_or("tree"))
            }
        }
    }

    /// Records that `id` is outside the subset; an error tree in its place.
    pub fn unsupported(&mut self, id: NodeId, what: &str) -> TNodeId {
        let span = self.ast.span(id);
        self.unsupported_at(span, what);
        self.error_tree(span)
    }

    pub fn unsupported_at(&mut self, span: Span, what: &str) {
        if self.unsupported.is_none() {
            self.unsupported = Some((span, what.to_owned()));
        }
    }

    pub fn error(&mut self, id: NodeId, msg: impl Into<String>) -> TNodeId {
        let span = self.ast.span(id);
        self.diags.error(span.point(), msg);
        self.error_tree(span)
    }

    fn error_tree(&mut self, span: Span) -> TNodeId {
        let name = TName { name: self.name("<error>"), is_type: false };
        let tpe = self.u.intern_type(Type::Error);
        self.tpd.node(T::Ident { name }, span, Some(tpe), None)
    }

    pub fn name(&mut self, text: &str) -> Name {
        self.names.intern(text)
    }

    pub fn text(&self, name: Name) -> &str {
        self.names.text(name)
    }
}
