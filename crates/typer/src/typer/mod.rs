//! Typing untyped trees into `tpd` trees (dotty's `Typer`): the dispatch over tree classes.

mod access;
mod adapt;
mod alternatives;
mod apply;
mod classes;
mod closures;
mod const_fold;
mod const_ops;
mod constructors;
mod control;
mod defs;
mod derived;
mod equality;
mod find_ref;
mod idents;
mod implicits;
mod import_tree;
mod imports;
mod infer;
mod literals;
mod names;
mod no_inits;
mod overloads;
mod overrides;
mod parent_trees;
mod parents;
mod patterns;
mod purity;
mod qualifies;
mod repeated;
mod report;
mod seen_from;
mod select;
mod stats;
mod this;
mod type_apply;
mod type_defs;
mod type_trees;
mod variances;
mod views;
mod widen_inferred;
mod widening;

use crate::ctx::{Ctx, LevelId, Levels};
use crate::defn::Defn;
use crate::symbols::Syms;
use crate::tpd::{TNodeId, Tpd};
use rustc_hash::FxHashMap;
use syntax::ast::{Ast, NodeId, Span, Tree};
use syntax::config::Features;
use syntax::diag::Diagnostics;
use syntax::names::Names;
use tasty::Universe;

pub use overloads::Pt;

pub struct Typer<'a> {
    pub u: &'a mut Universe,
    pub ast: &'a mut Ast,
    pub names: &'a mut Names,
    pub defn: Defn,
    pub tpd: Tpd,
    pub levels: Levels,
    pub syms: Syms,
    pub diags: Diagnostics,
    /// The language features the options and the unit's language imports enable.
    pub features: Features,
    /// Trees typed while completing a symbol, taken when typing reaches them.
    pub ahead: FxHashMap<NodeId, TNodeId>,
    /// Untyped trees that stand for typed ones (dotty's `TypedSplice`).
    pub splices: FxHashMap<NodeId, TNodeId>,
    /// The imports of the unit, root imports first, by `Level::Import` index.
    pub imports: Vec<imports::ImportEntry>,
    /// The level each import tree opens.
    pub import_levels: FxHashMap<NodeId, LevelId>,
    /// The bounds and instances of the type variables.
    pub constraint: crate::constraint::Constraint,
    /// How many more candidates the implicit search in progress may try.
    pub implicit_budget: u32,
    /// The argument lists of the applications being typed.
    pub protos: Vec<overloads::FunProto>,
    /// The first construct outside the subset, and where.
    pub unsupported: Option<(Span, String)>,
    /// How many `typed` calls are in progress, one inside another.
    depth: u32,
}

/// How many `typed` calls may be in progress at once.
const MAX_DEPTH: u32 = 400;

impl<'a> Typer<'a> {
    pub fn new(
        u: &'a mut Universe,
        ast: &'a mut Ast,
        names: &'a mut Names,
        features: Features,
    ) -> Result<Self, String> {
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
            features,
            ahead: FxHashMap::default(),
            splices: FxHashMap::default(),
            imports: Vec::new(),
            import_levels: FxHashMap::default(),
            constraint: Default::default(),
            implicit_budget: 0,
            protos: Vec::new(),
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
        let vars = self.constraint.count();
        let tree = match self.ready(id) {
            Some(t) => t,
            None => self.typed_unadapted(id, pt, ctx),
        };
        if let Err(what) = self.interpolate(tree, vars) {
            self.unsupported(id, what);
        }
        if let Some(t) = self.tpd.get(tree).tpe {
            let t = self.zonk(t);
            self.tpd.get_mut(tree).tpe = Some(t);
        }
        let tree = self.adapt(tree, pt, ctx);
        self.depth -= 1;
        tree
    }

    fn typed_unadapted(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        match self.ast.tree(id).clone() {
            Tree::Ident { name } => self.typed_ident(id, name, pt, ctx),
            Tree::Select { qualifier, name } => self.typed_select(id, qualifier, name, pt, ctx),
            Tree::Apply { fun, args } => self.typed_apply(id, fun, args, pt, ctx),
            Tree::TypeApply { fun, args } => self.typed_type_apply(id, fun, args, ctx),
            Tree::Literal { value } => self.typed_literal(id, value),
            Tree::Number { digits, kind } => self.typed_number(id, &digits, kind, pt),
            Tree::TypedSplice { sym } => self.typed_full_ref(id, sym),
            Tree::This { qual } => self.typed_this(id, qual, ctx),
            Tree::New { tpt } => self.typed_new(id, tpt, ctx),
            Tree::Typed { expr, tpt } => self.typed_typed(id, expr, tpt, pt, ctx),
            Tree::Block { stats, expr } => self.typed_block(id, stats, expr, pt, ctx),
            Tree::Try { expr, cases, finalizer: None } => self.typed_try(id, expr, cases, pt, ctx),
            Tree::If { cond, thenp, elsep, inline: false } => self.typed_if(id, cond, thenp, elsep, pt, ctx),
            Tree::WhileDo { cond, body } => self.typed_while(id, cond, body, ctx),
            Tree::Assign { lhs, rhs } => self.typed_assign(id, lhs, rhs, ctx),
            Tree::Return { expr } => self.typed_return(id, expr, ctx),
            Tree::Throw { expr } => self.typed_throw(id, expr, ctx),
            Tree::Bind { name, body, .. } => self.typed_bind(id, name, body, pt, ctx),
            Tree::Function { args, body, mods: None, .. } => self.typed_function(id, args, body, pt, ctx),
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
}
