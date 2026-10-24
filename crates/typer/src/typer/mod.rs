// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Typing untyped trees into `tpd` trees (dotty's `Typer`): the dispatch over tree classes.

mod access;
mod accessible;
mod accessor_defs;
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
pub(crate) mod approx;
mod arg_shapes;
mod assign;
mod auto_apply;
mod avoid;
mod avoid_captures;
mod avoid_map;
mod avoid_select;
mod by_name_repeated;
mod call_types;
mod case_companion;
mod case_lambdas;
mod clashing_synthetics;
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
mod define_further;
mod defs;
mod dependent_function_types;
mod dependent_functions;
pub(crate) mod dependent_params;
mod dependent_results;
mod derived;
mod deriving;
mod deriving_finalize;
mod deriving_kinds;
mod deriving_trees;
pub(crate) mod deskolemize;
pub(crate) mod did_you_mean;
mod dispatch;
mod dynamic;
mod enum_case_refs;
mod enum_cases;
mod enum_refs;
mod equality;
mod errors;
mod escaping;
mod eta;
mod export_trees;
mod extension_calls;
mod extension_fallback;
mod features;
mod find_ref;
mod function_types;
mod future_infix;
mod generic_arrays;
mod harmonize;
mod ident_hint;
mod idents;
mod implicit_views;
mod implicits;
mod import_selectors;
mod import_tree;
mod imports;
mod infer;
mod infix_tuples;
mod inline_accessors;
mod inline_host;
mod inline_refs;
mod interpolate;
mod irrefutable;
mod java_classes;
mod java_enums;
mod java_members;
mod java_objects;
mod jdk_members;
mod lift;
mod lift_app;
mod linter;
mod literals;
mod mapped_alternatives;
mod match_type_trees;
mod matchable_cast;
mod matches;
mod maximize;
mod member_hint;
mod member_views;
mod members;
mod migrations;
mod mismatch_context;
mod mismatch_text;
mod name_clashes;
mod named_args;
mod named_overloads;
mod named_tuple_types;
mod named_tuples;
mod named_type_args;
mod names;
mod no_inits;
mod non_method_apply;
mod not_a_member;
mod not_found;
mod op_assign;
mod overload_errors;
mod overloaded_extractors;
mod overloads;
mod overrides;
mod package_members;
mod param_vars;
mod parameterless;
mod parent_trees;
mod parents;
mod paths;
mod pattern_args;
mod pattern_bounds;
mod patterns;
mod plain_trees;
mod poly_alternatives;
mod poly_context_bounds;
mod poly_function_values;
mod poly_functions;
mod poly_instances;
mod pretyped_args;
mod purity;
mod qualifier_views;
mod qualifies;
mod refined_types;
mod refinement_members;
mod refutable_warnings;
mod repeated;
mod report;
mod retained;
mod right_assoc;
mod sam_closures;
mod sams;
mod scope_bindings;
pub(crate) mod seen_from;
mod select;
mod selves;
mod shown_prefix;
pub(crate) mod simplify;
mod small_tuples;
mod source_splices;
mod spliced_args;
pub(crate) mod stability;
mod stats;
mod structural;
mod summon_from;
mod supers;
mod this;
mod this_calls;
mod trait_params;
mod transparent;
mod transparent_traits;
mod tries;
mod try_handlers;
mod tuple_xxl;
pub(crate) mod type_apply;
mod type_args;
mod type_arity;
mod type_defs;
mod type_lambdas;
mod type_mismatch;
mod type_tests;
mod type_trees;
mod unapply;
mod unapply_implicits;
mod unapply_results;
mod unapply_seq;
mod unapply_targs;
mod unchecked_tests;
mod unknown_params;
mod untupled;
mod variances;
mod views;
mod widen_inferred;
mod widen_union;
mod widening;
mod wild_approx;
mod wildcard_aliases;
mod wildcard_members;

use crate::ctx::{Ctx, LevelId, Levels};
use crate::defn::{Defn, entered};
use crate::symbols::Syms;
use crate::tpd::{TNodeId, Tpd};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::{Ast, NodeId, Span};
use syntax::config::{Config, Features, Lints, SourceVersion};
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use tasty::{Kind, SymbolId, Universe};

pub use const_ops::{fold_binop, fold_unop};
pub use overloads::Pt;
pub use retained::RETAINED_BODY;
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
    /// The warnings dotty reports after the typer, in erasure.
    pub erasure_diags: Diagnostics,
    /// The language features the options and the unit's language imports enable.
    pub features: Features,
    /// The `-W` lints the options enable.
    pub lints: Lints,
    /// Whether the source version is `future`.
    pub source_future: bool,
    /// Trees typed while completing a symbol, by the owner they were typed in, taken when typing
    /// reaches them.
    pub ahead: FxHashMap<NodeId, Vec<(SymbolId, TNodeId)>>,
    /// Untyped trees that stand for typed ones (dotty's `TypedSplice`).
    pub splices: FxHashMap<NodeId, TNodeId>,
    /// The owners of spliced trees, which their definitions leave when spliced (`TypedSplice.owner`).
    pub splice_owners: FxHashMap<NodeId, SymbolId>,
    /// The `new`s of parent calls (dotty's `noCheckNews`).
    pub parent_news: rustc_hash::FxHashSet<NodeId>,
    /// The imports of the unit, root imports first, by `Level::Import` index.
    pub imports: Vec<imports::ImportEntry>,
    /// The level each import tree opens.
    pub import_levels: FxHashMap<NodeId, LevelId>,
    /// The level of each nested package clause, whose statements are indexed with the enclosing ones.
    pub package_levels: FxHashMap<NodeId, LevelId>,
    /// The bounds and instances of the type variables.
    pub constraint: crate::constraint::Constraint,
    /// Type variables numbered below this are not instantiated before implicit searches (dotty's
    /// `locked`).
    pub locked_vars: usize,
    /// How many more candidates the implicit search in progress may try.
    pub implicit_budget: u32,
    /// The implicit searches in progress and their by-name implicits.
    pub history: implicits::History,
    /// The argument lists of the applications being typed.
    pub protos: Vec<overloads::FunProto>,
    /// The type argument lists of the type applications being typed.
    pub poly_protos: Vec<poly_alternatives::PolyProto>,
    /// The alternatives being resolved with their type parameters instantiated, and the types.
    pub mapped_targs: Option<(Vec<SymbolId>, Vec<tasty::TypeId>)>,
    /// The copies of alternatives resolved by a later clause: the original and how many clauses
    /// the copy skips.
    pub mapped_alts: FxHashMap<SymbolId, (SymbolId, usize)>,
    /// The applicable alternatives of the last overload resolution found ambiguous.
    pub ambiguous: Vec<SymbolId>,
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
    pub accessors: inline_accessors::Accessors,
    /// The implicit members of the unit's package objects in each package, once asked for.
    pub package_implicits: FxHashMap<SymbolId, Vec<(SymbolId, SymbolId)>>,
    /// The forwarders of each export clause in a class body.
    pub exports: FxHashMap<NodeId, crate::namer::exports::Exported>,
    /// The definitions of the instances of each module class's `derives` clause, entered and
    /// not typed yet.
    pub derived_instances: FxHashMap<SymbolId, Vec<(NodeId, Option<deriving_finalize::Problem>)>>,
    /// The type lambdas of derived instances' types, by the instances that own their parameters.
    pub derived_lambdas: FxHashMap<NodeId, NodeId>,
    /// The pending dynamic calls, by their placeholder trees.
    pub dynamic_calls: FxHashMap<TNodeId, dynamic::DynamicCall>,
    /// The left-hand side of the assignment being typed.
    pub dynamic_lhs: Option<NodeId>,
    /// The functions of polymorphic function values, whose results may refer to their
    /// parameters, with the binder and parameter names of an expected `apply` whose result does.
    pub poly_closures: FxHashMap<NodeId, Option<(tasty::Binder, Vec<String>)>>,
    /// The parameters of context closures named after an expected `apply`'s, not in scope.
    pub hidden_params: FxHashSet<NodeId>,
    /// How many `typed` calls are in progress, one inside another.
    depth: u32,
    /// The offset each source file of the unit starts at, after the first, which starts at 0.
    pub file_starts: Vec<syntax::source::Offset>,
    /// What runs macro implementations; without it, macro calls are refused.
    pub macros: Option<&'a mut dyn crate::inlining::Macros>,
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
        let v = cfg.source_version;
        if !v.is_at_least(SourceVersion::V3_7) || v.migrating {
            return Err("source versions other than 3.7 to 3.9".into());
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
            erasure_diags: Diagnostics::default(),
            features: cfg.features,
            lints: cfg.warnings.lints,
            source_future: v.requires_new_syntax(),
            ahead: FxHashMap::default(),
            splices: FxHashMap::default(),
            splice_owners: FxHashMap::default(),
            parent_news: Default::default(),
            imports: Vec::new(),
            import_levels: FxHashMap::default(),
            package_levels: FxHashMap::default(),
            constraint: Default::default(),
            locked_vars: 0,
            implicit_budget: 0,
            history: Default::default(),
            protos: Vec::new(),
            poly_protos: Vec::new(),
            mapped_targs: None,
            mapped_alts: FxHashMap::default(),
            ambiguous: Vec::new(),
            unsupported: None,
            reports: 0,
            method_infos: Default::default(),
            probe: None,
            inline_defs: FxHashMap::default(),
            inline_ctxs: FxHashMap::default(),
            accessors: Default::default(),
            package_implicits: FxHashMap::default(),
            exports: FxHashMap::default(),
            derived_instances: FxHashMap::default(),
            derived_lambdas: Default::default(),
            dynamic_calls: FxHashMap::default(),
            dynamic_lhs: None,
            poly_closures: FxHashMap::default(),
            hidden_params: FxHashSet::default(),
            depth: 0,
            file_starts: Vec::new(),
            macros: None,
        };
        typer.enter_and_or();
        typer.u.set_comparison_owner(None);
        Ok(typer)
    }

    /// The index of the source file of the unit that `span` is in.
    pub fn file_of(&self, span: Span) -> usize {
        self.file_starts.partition_point(|&s| s <= span.in_own_source().without_inlined_call().start())
    }

    pub fn typed(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if self.depth >= MAX_DEPTH {
            return self.unsupported(id, "nesting too deep");
        }
        self.depth += 1;
        let outer = self.u.set_comparison_owner(Some(ctx.owner));
        let vars = self.constraint.count();
        let top_only = crate::ctx::Mode::APPLY_QUALIFIER.with(crate::ctx::Mode::ASSIGNMENT_LHS);
        let inner = ctx.with_mode(ctx.mode.without(top_only));
        let tree = match self.contextual_function(id, pt, inner).or_else(|| self.ready(id, inner)) {
            Some(t) => t,
            None => self.typed_unadapted(id, pt, inner),
        };
        if let Err(what) = self.interpolate(tree, pt, vars) {
            self.unsupported(id, what);
        }
        self.settle_type(tree, inner);
        let locked = std::mem::replace(&mut self.locked_vars, vars);
        let adapted = self.adapt(tree, pt, ctx);
        self.locked_vars = locked;
        if let Err(what) = self.reinterpolate((tree, adapted), pt, vars) {
            self.unsupported(id, what);
        }
        self.u.set_comparison_owner(outer);
        self.depth -= 1;
        adapted
    }
}
