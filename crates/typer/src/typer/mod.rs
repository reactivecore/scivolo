//! Typing untyped trees into `tpd` trees (dotty's `Typer`): the dispatch over tree classes.

mod access;
mod adapt;
mod adapt_by_result;
mod ahead;
mod alternatives;
mod and_members;
mod and_or_types;
mod annotated;
mod anon_classes;
mod applicability;
mod apply;
mod apply_insertion;
mod arg_shapes;
mod auto_apply;
mod avoid;
mod avoid_pattern_types;
mod call_types;
mod case_lambdas;
mod class_bound;
mod classes;
mod closures;
mod const_fold;
pub(crate) mod const_ops;
mod constants;
mod constrain_pattern;
mod constructor_proxies;
mod constructors;
mod context_bounds;
mod context_functions;
mod control;
mod default_args;
mod default_params;
mod defs;
pub(crate) mod dependent_params;
mod derived;
pub(crate) mod deskolemize;
pub(crate) mod did_you_mean;
mod dispatch;
mod enum_case_refs;
mod enum_cases;
mod equality;
mod errors;
mod escaping;
mod eta;
mod export_trees;
mod extension_calls;
mod features;
mod find_ref;
mod function_types;
mod generic_arrays;
mod harmonize;
mod idents;
mod implicit_views;
mod implicits;
mod import_tree;
mod imports;
mod infer;
mod infix_tuples;
mod inline_host;
mod inline_refs;
mod interpolate;
mod irrefutable;
mod java_classes;
mod java_members;
mod java_objects;
mod lift;
mod lift_app;
mod linter;
mod literals;
mod matches;
mod member_views;
mod members;
mod migrations;
mod mismatch_context;
mod mismatch_text;
mod name_clashes;
mod named_args;
mod names;
mod no_inits;
mod not_a_member;
mod not_found;
mod op_assign;
mod overloads;
mod overrides;
mod param_vars;
mod parent_trees;
mod parents;
mod paths;
mod pattern_args;
mod pattern_bounds;
mod patterns;
mod plain_trees;
mod pretyped_args;
mod purity;
mod qualifier_views;
mod qualifies;
mod refined_types;
mod refutable_warnings;
mod repeated;
mod report;
mod right_assoc;
mod sam_closures;
mod sams;
pub(crate) mod seen_from;
mod select;
mod selves;
mod small_tuples;
pub(crate) mod stability;
mod stats;
mod summon_from;
mod supers;
mod this;
mod this_calls;
mod transparent;
mod transparent_traits;
mod tries;
pub(crate) mod type_apply;
mod type_args;
mod type_defs;
mod type_lambdas;
mod type_mismatch;
mod type_trees;
mod unapply;
mod unapply_results;
mod unapply_seq;
mod unknown_params;
mod untupled;
mod variances;
mod views;
mod widen_inferred;
mod widen_union;
mod widening;
mod wild_approx;

use crate::ctx::{Ctx, LevelId, Levels};
use crate::defn::{Defn, entered};
use crate::symbols::Syms;
use crate::tpd::{TNodeId, Tpd};
use rustc_hash::FxHashMap;
use syntax::ast::{Ast, NodeId, Span};
use syntax::config::{Config, Features, Lints, SourceVersion};
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use tasty::{Kind, SymbolId, Universe};

pub use const_ops::{fold_binop, fold_unop};
pub use overloads::Pt;
pub use transparent::InlineDef;
pub use widen_inferred::Widen;

pub struct Typer<'a> {
    pub u: &'a mut Universe,
    pub ast: &'a mut Ast,
    /// The root tree of the unit.
    pub unit: NodeId,
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
    /// The `-W` lints the options enable.
    pub lints: Lints,
    /// Trees typed while completing a symbol, by the owner they were typed in, taken when typing
    /// reaches them.
    pub ahead: FxHashMap<NodeId, Vec<(SymbolId, TNodeId)>>,
    /// Untyped trees that stand for typed ones (dotty's `TypedSplice`).
    pub splices: FxHashMap<NodeId, TNodeId>,
    /// The owners of spliced trees, which their definitions leave when spliced (`TypedSplice.owner`).
    pub splice_owners: FxHashMap<NodeId, SymbolId>,
    /// The imports of the unit, root imports first, by `Level::Import` index.
    pub imports: Vec<imports::ImportEntry>,
    /// The level each import tree opens.
    pub import_levels: FxHashMap<NodeId, LevelId>,
    /// The level of each nested package clause, whose statements are indexed with the enclosing ones.
    pub package_levels: FxHashMap<NodeId, LevelId>,
    /// The bounds and instances of the type variables.
    pub constraint: crate::constraint::Constraint,
    /// How many more candidates the implicit search in progress may try.
    pub implicit_budget: u32,
    /// The implicit searches in progress and their by-name implicits.
    pub history: implicits::History,
    /// The argument lists of the applications being typed.
    pub protos: Vec<overloads::FunProto>,
    /// The first construct outside the subset, and where.
    pub unsupported: Option<(Span, String)>,
    /// How many constructs outside the subset were reported.
    reports: u32,
    /// `method_info`'s results in universe generation `.0`.
    method_infos: (u64, FxHashMap<tasty::TypeId, Option<tasty::TypeId>>),
    /// What a completion request records while typing.
    pub probe: Option<crate::completion::Probe>,
    /// The definitions of the unit's inline methods.
    pub inline_defs: FxHashMap<SymbolId, InlineDef>,
    /// The context of each call of an inline method outside one.
    pub inline_ctxs: FxHashMap<TNodeId, Ctx>,
    /// The forwarders of each export clause in a class body.
    pub exports: FxHashMap<NodeId, crate::namer::exports::Exported>,
    /// How many `typed` calls are in progress, one inside another.
    depth: u32,
}

/// How many `typed` calls may be in progress at once.
const MAX_DEPTH: u32 = 400;

impl<'a> Typer<'a> {
    pub fn new(
        u: &'a mut Universe,
        (ast, unit): (&'a mut Ast, NodeId),
        (names, fresh): (&'a mut Names, &'a mut FreshNames),
        cfg: Config,
        syms: &mut Syms,
    ) -> Result<Self, String> {
        if cfg.source_version != SourceVersion::V3_9 {
            return Err("source versions other than 3.9".into());
        }
        let defn = Defn::new(u)?;
        let mut syms = std::mem::take(syms);
        let ops = u.package("<special-ops>");
        syms.throw_method = syms.throw_method.or_else(|| entered(u, ops?, "throw", Kind::Def));
        syms.repeated_class =
            syms.repeated_class.or_else(|| entered(u, defn.scala, "<repeated>", Kind::Class));
        let mut typer = Typer {
            u,
            ast,
            unit,
            names,
            fresh,
            defn,
            tpd: Tpd::default(),
            levels: Levels::default(),
            syms,
            diags: Diagnostics::default(),
            features: cfg.features,
            lints: cfg.warnings.lints,
            ahead: FxHashMap::default(),
            splices: FxHashMap::default(),
            splice_owners: FxHashMap::default(),
            imports: Vec::new(),
            import_levels: FxHashMap::default(),
            package_levels: FxHashMap::default(),
            constraint: Default::default(),
            implicit_budget: 0,
            history: Default::default(),
            protos: Vec::new(),
            unsupported: None,
            reports: 0,
            method_infos: Default::default(),
            probe: None,
            inline_defs: FxHashMap::default(),
            inline_ctxs: FxHashMap::default(),
            exports: FxHashMap::default(),
            depth: 0,
        };
        typer.enter_and_or();
        typer.u.set_comparison_owner(None);
        Ok(typer)
    }

    pub fn typed(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if self.depth >= MAX_DEPTH {
            return self.unsupported(id, "nesting too deep");
        }
        self.depth += 1;
        let outer = self.u.set_comparison_owner(Some(ctx.owner));
        let vars = self.constraint.count();
        let inner = ctx.with_mode(ctx.mode.without(crate::ctx::Mode::APPLY_QUALIFIER));
        let tree = match self.contextual_function(id, pt, inner).or_else(|| self.ready(id, inner)) {
            Some(t) => t,
            None => self.typed_unadapted(id, pt, inner),
        };
        if let Err(what) = self.interpolate(tree, pt, vars) {
            self.unsupported(id, what);
        }
        self.settle_type(tree, inner);
        let tree = self.adapt(tree, pt, ctx);
        self.u.set_comparison_owner(outer);
        self.depth -= 1;
        tree
    }
}
