//! Typing untyped trees into `tpd` trees (dotty's `Typer`): the dispatch over tree classes.

mod access;
mod adapt;
mod ahead;
mod alternatives;
mod and_or_types;
mod anon_classes;
mod applicability;
mod apply;
mod avoid;
mod classes;
mod closures;
mod const_fold;
mod const_ops;
mod constructor_proxies;
mod constructors;
mod control;
mod default_args;
mod defs;
mod dependent_params;
mod derived;
mod did_you_mean;
mod dispatch;
mod enum_case_refs;
mod enum_cases;
mod equality;
mod errors;
mod eta;
mod find_ref;
mod function_types;
mod idents;
mod implicits;
mod import_tree;
mod imports;
mod infer;
mod java_classes;
mod lift;
mod literals;
mod matches;
mod members;
mod named_args;
mod names;
mod no_inits;
mod not_found;
mod op_assign;
mod overloads;
mod overrides;
mod parent_trees;
mod parents;
mod patterns;
mod purity;
mod qualifies;
mod repeated;
mod report;
mod right_assoc;
mod seen_from;
mod select;
mod stability;
mod stats;
mod this;
mod tries;
mod type_apply;
mod type_defs;
mod type_trees;
mod unapply;
mod unapply_results;
mod unapply_seq;
mod unknown_params;
mod untupled;
mod variances;
mod views;
mod widen_inferred;
mod widening;

use crate::ctx::{Ctx, LevelId, Levels};
use crate::defn::{Defn, entered};
use crate::symbols::Syms;
use crate::tpd::{TNodeId, Tpd};
use rustc_hash::FxHashMap;
use syntax::ast::{Ast, NodeId, Span};
use syntax::config::Features;
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use tasty::{Kind, SymbolId, Universe};

pub use overloads::Pt;

pub struct Typer<'a> {
    pub u: &'a mut Universe,
    pub ast: &'a mut Ast,
    pub names: &'a mut Names,
    /// The counters of the unit's fresh names, shared with the desugaring.
    pub fresh: &'a mut FreshNames,
    pub defn: Defn,
    pub tpd: Tpd,
    pub levels: Levels,
    pub syms: Syms,
    pub diags: Diagnostics,
    /// The language features the options and the unit's language imports enable.
    pub features: Features,
    /// Trees typed while completing a symbol, by the owner they were typed in, taken when typing
    /// reaches them.
    pub ahead: FxHashMap<NodeId, Vec<(SymbolId, TNodeId)>>,
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
        (names, fresh): (&'a mut Names, &'a mut FreshNames),
        features: Features,
        syms: &mut Syms,
    ) -> Result<Self, String> {
        let defn = Defn::new(u)?;
        let mut syms = std::mem::take(syms);
        let ops = u.package("<special-ops>");
        syms.throw_method = syms.throw_method.or_else(|| entered(u, ops?, "throw", Kind::Def));
        syms.repeated_class =
            syms.repeated_class.or_else(|| entered(u, defn.scala, "<repeated>", Kind::Class));
        let mut typer = Typer {
            u,
            ast,
            names,
            fresh,
            defn,
            tpd: Tpd::default(),
            levels: Levels::default(),
            syms,
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
        };
        typer.enter_and_or();
        Ok(typer)
    }

    pub fn typed(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if self.depth >= MAX_DEPTH {
            return self.unsupported(id, "nesting too deep");
        }
        self.depth += 1;
        let vars = self.constraint.count();
        let tree = match self.ready(id, ctx) {
            Some(t) => t,
            None => self.typed_unadapted(id, pt, ctx),
        };
        if let Err(what) = self.interpolate(tree, vars) {
            self.unsupported(id, what);
        }
        self.settle_type(tree, ctx);
        let tree = self.adapt(tree, pt, ctx);
        self.depth -= 1;
        tree
    }
}
