// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Typing untyped trees into `tpd` trees (dotty's `Typer`): the dispatch over tree classes.

mod abstract_members;
mod access;
mod accessible;
mod accessor_defs;
mod accessor_types;
mod adapt;
mod adapt_by_result;
mod ahead;
mod alias_projections;
mod alternatives;
mod and_members;
mod and_or_types;
mod annotated;
mod anon_classes;
mod applicability;
mod apply;
mod apply_insertion;
pub mod approx;
mod arg_shapes;
mod ascriptions;
mod assign;
mod auto_apply;
mod avoid;
mod avoid_captures;
mod avoid_map;
mod avoid_select;
mod by_name_repeated;
mod call_types;
mod callee_types;
mod capture_types;
mod captured_paths;
mod case_companion;
mod case_lambdas;
mod case_nulls;
mod clashing_synthetics;
mod class_bound;
mod class_members;
mod classes;
mod closure_params;
mod closure_trees;
mod closures;
mod completion_diags;
mod const_fold;
pub(crate) mod const_ops;
mod constants;
mod constrain_pattern;
mod constructor_params;
pub(crate) mod constructor_proxies;
mod constructors;
mod context_applications;
mod context_bounds;
mod context_functions;
mod control;
mod default_args;
mod default_params;
mod deferred_givens;
mod define_further;
mod defs;
mod dependent_function_types;
mod dependent_functions;
mod dependent_matches;
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
mod erased_function_types;
mod errors;
mod escaping;
mod eta;
mod export_trees;
mod extension_calls;
mod extension_fallback;
mod features;
mod find_ref;
mod forward_refs;
mod function_types;
mod future_infix;
mod gadt;
mod gadt_cases;
mod gadt_cast;
mod gadt_narrow;
mod gadt_order;
mod gadt_tuples;
mod gadt_typed_pattern;
mod gadt_upcast;
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
mod inherited_conflicts;
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
mod member_hiding;
mod member_hint;
mod member_matching;
mod member_views;
mod members;
mod migrations;
mod mismatch_context;
mod mismatch_text;
mod name_clashes;
mod named_args;
mod named_overloads;
mod named_tuple_trees;
mod named_tuple_types;
mod named_tuples;
mod named_type_args;
mod names;
mod new_type_args;
mod no_inits;
mod non_method_apply;
mod not_a_member;
mod not_found;
mod null_assignments;
mod null_conditionals;
mod null_infos;
mod null_loops;
pub(crate) mod nullables;
mod op_assign;
mod opaque_paths;
mod overload_applies;
mod overload_errors;
mod overload_values;
mod overloaded_extractors;
mod overloads;
mod override_kinds;
mod overrides;
mod overrides_nothing;
mod package_members;
mod param_vars;
mod parameterless;
mod parent_trees;
mod parents;
mod passing_accessors;
mod passing_defs;
mod paths;
mod pattern_alternatives;
mod pattern_args;
mod pattern_bounds;
mod pattern_gadt;
mod patterns;
mod plain_trees;
mod poly_alternatives;
mod poly_context_bounds;
mod poly_function_values;
mod poly_functions;
mod poly_instances;
mod pretyped_args;
mod purity;
mod qual_skolems;
mod qualifier_views;
mod qualifies;
mod quote_pattern_checks;
mod quote_patterns;
mod quote_tags;
mod quote_type_bindings;
mod refined_projections;
mod refined_types;
mod refinement_members;
mod refinement_scope;
mod refutable_warnings;
mod repeated;
mod report;
mod retained;
mod right_assoc;
mod sam_closures;
mod sam_method_values;
mod sams;
mod scala2_macros;
mod scope_bindings;
pub(crate) mod seen_from;
mod select;
mod selectable_fields;
mod selves;
mod shown_prefix;
pub(crate) mod simplify;
mod small_tuples;
mod source_splices;
mod splice_patterns;
mod spliced_args;
mod spreads;
pub(crate) mod stability;
mod stats;
mod structural;
mod structural_calls;
mod summon_from;
mod supers;
mod this;
mod this_calls;
mod throws_types;
mod top_level_annotations;
mod tracked_refs;
mod trait_params;
mod transparent;
mod transparent_traits;
mod tries;
mod try_capabilities;
mod try_handlers;
mod tuple_xxl;
mod tupled_cases;
pub(crate) mod type_apply;
mod type_args;
mod type_arity;
mod type_checks;
mod type_defs;
mod type_lambdas;
mod type_mismatch;
mod type_reductions;
mod type_tests;
mod type_trees;
mod unapply;
mod unapply_fun;
mod unapply_implicits;
mod unapply_results;
mod unapply_seq;
mod unapply_targs;
mod unapply_transparent;
mod unchecked_tests;
pub(crate) mod unique_names;
mod unknown_params;
mod unresolved_terms;
mod unsafe_nulls;
mod untupled;
mod variance_aliases;
mod variance_checks;
mod variance_lambdas;
mod variance_messages;
mod variance_types;
mod variances;
mod views;
mod widen_inferred;
mod widen_union;
mod widening;
mod wild_approx;
mod wildcard_aliases;
mod wildcard_members;
mod zonk_trees;

use crate::ctx::{Ctx, LevelId, Levels};
use crate::defn::Defn;
use crate::special::entered;
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
    /// The diagnostics completions reported.
    pub completion_diags: Vec<syntax::diag::Diagnostic>,
    /// The warnings dotty reports after the typer, in erasure.
    pub erasure_diags: Diagnostics,
    /// The language features the options and the unit's language imports enable.
    pub features: Features,
    /// `-Yexplicit-nulls`.
    pub explicit_nulls: bool,
    pub nullables: nullables::Nullables,
    /// The setting that makes the top-level definitions experimental (`Config::experimental`).
    pub experimental: Option<&'static str>,
    /// The `-W` lints the options enable.
    pub lints: Lints,
    /// Whether the source version is `future`.
    pub source_future: bool,
    /// Whether the unit behaves as the Scala 2 library (`language.\`2.13\``).
    pub scala2: bool,
    /// Trees typed while completing a symbol, by the owner they were typed in, taken when typing
    /// reaches them.
    pub ahead: FxHashMap<NodeId, Vec<(SymbolId, TNodeId)>>,
    /// Untyped trees that stand for typed ones (dotty's `TypedSplice`).
    pub splices: FxHashMap<NodeId, TNodeId>,
    /// The owners of spliced trees, which their definitions leave when spliced (`TypedSplice.owner`).
    pub splice_owners: FxHashMap<NodeId, SymbolId>,
    /// The spliced receivers of extension method calls (`TypedSplice.isExtensionReceiver`).
    pub extension_receivers: rustc_hash::FxHashSet<NodeId>,
    /// The `new`s of parent calls (dotty's `noCheckNews`).
    pub parent_news: rustc_hash::FxHashSet<NodeId>,
    /// The traits whose class parent was taken as `Object` while their parents were completed.
    pub object_parents_assumed: Vec<SymbolId>,
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
    /// The candidates the next outermost search of an implicit argument leaves out.
    pub ignored_implicits: Vec<SymbolId>,
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
    /// The code of `typeChecks` being typed, and the number of diagnostics at its first construct
    /// outside the subset.
    pub checked_code: Option<(NodeId, Option<usize>)>,
    /// The errors inline expansions and checks with dotty's messages reported, with their spans.
    pub expansion_errors: Vec<(Span, String)>,
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
    /// The context of each sequence literal with spreads.
    pub spread_ctxs: FxHashMap<TNodeId, Ctx>,
    /// The private variables assigned other than through their owner's `this` (dotty's
    /// `AssignedNonLocally`).
    pub assigned_non_locally: FxHashSet<SymbolId>,
    /// The variances of the parameters of each alias's type lambda, once asked for; all +1 while
    /// they are computed.
    pub alias_variances: FxHashMap<tasty::Binder, Vec<i8>>,
    pub accessors: inline_accessors::Accessors,
    /// The implicit members of the unit's package objects in each package, once asked for.
    pub package_implicits: FxHashMap<SymbolId, Vec<(SymbolId, SymbolId)>>,
    /// The forwarders of each export clause in a class body.
    pub exports: FxHashMap<NodeId, crate::namer::exports::Exported>,
    /// The definitions of the instances of each module class's `derives` clause, entered and
    /// not typed yet.
    pub derived_instances: FxHashMap<SymbolId, Vec<(NodeId, Option<deriving_finalize::Problem>)>>,
    /// The types typed patterns bind whose bounds the selector gave.
    pub known_pattern_types: FxHashSet<SymbolId>,
    pub gadt: gadt_cases::Gadt,
    /// The type lambdas of derived instances' types, by the instances that own their parameters.
    pub derived_lambdas: FxHashMap<NodeId, NodeId>,
    /// The pending dynamic calls, by their placeholder trees.
    pub dynamic_calls: FxHashMap<TNodeId, dynamic::DynamicCall>,
    /// The selections of structural methods not rewritten to `applyDynamic` calls yet.
    pub structural_selects: FxHashSet<TNodeId>,
    /// The left-hand side of the assignment being typed.
    pub dynamic_lhs: Option<NodeId>,
    /// The functions of polymorphic function values, whose results may refer to their
    /// parameters, with the binder and parameter names of an expected `apply` whose result does.
    pub poly_closures: FxHashMap<NodeId, Option<(tasty::Binder, Vec<String>)>>,
    /// The owners `poly_apply_owner` made.
    pub poly_owners: FxHashSet<SymbolId>,
    /// The parameters of context closures named after an expected `apply`'s, not in scope.
    pub hidden_params: FxHashSet<NodeId>,
    /// The classes with macro annotations, their `this` types and spans, checked for unimplemented
    /// members once the annotations expanded.
    pub macro_annotated: Vec<(SymbolId, tasty::TypeId, Span)>,
    /// The quote pattern being typed.
    pub quote_pattern: Option<quote_patterns::QuotePatternScope>,
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
        u.reductions_exhausted = false;
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
            completion_diags: Vec::new(),
            erasure_diags: Diagnostics::default(),
            features: cfg.features,
            explicit_nulls: cfg.explicit_nulls,
            nullables: Default::default(),
            experimental: cfg.experimental,
            lints: cfg.warnings.lints,
            source_future: v.requires_new_syntax(),
            scala2: v.is_scala2(),
            ahead: FxHashMap::default(),
            splices: FxHashMap::default(),
            splice_owners: FxHashMap::default(),
            extension_receivers: Default::default(),
            parent_news: Default::default(),
            object_parents_assumed: Vec::new(),
            imports: Vec::new(),
            import_levels: FxHashMap::default(),
            package_levels: FxHashMap::default(),
            constraint: Default::default(),
            locked_vars: 0,
            implicit_budget: 0,
            ignored_implicits: Vec::new(),
            history: Default::default(),
            protos: Vec::new(),
            poly_protos: Vec::new(),
            mapped_targs: None,
            mapped_alts: FxHashMap::default(),
            ambiguous: Vec::new(),
            unsupported: None,
            checked_code: None,
            expansion_errors: Vec::new(),
            reports: 0,
            method_infos: Default::default(),
            probe: None,
            inline_defs: FxHashMap::default(),
            inline_ctxs: FxHashMap::default(),
            spread_ctxs: FxHashMap::default(),
            assigned_non_locally: FxHashSet::default(),
            alias_variances: FxHashMap::default(),
            accessors: Default::default(),
            package_implicits: FxHashMap::default(),
            exports: FxHashMap::default(),
            derived_instances: FxHashMap::default(),
            known_pattern_types: FxHashSet::default(),
            gadt: Default::default(),
            derived_lambdas: Default::default(),
            dynamic_calls: FxHashMap::default(),
            structural_selects: FxHashSet::default(),
            dynamic_lhs: None,
            poly_closures: FxHashMap::default(),
            poly_owners: FxHashSet::default(),
            hidden_params: FxHashSet::default(),
            macro_annotated: Vec::new(),
            quote_pattern: None,
            depth: 0,
            file_starts: Vec::new(),
            macros: None,
        };
        typer.tpd.scala2 = typer.scala2;
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
        self.type_uniques(id);
        let outer = self.u.set_comparison_owner(Some(ctx.owner));
        let nulls = self.null_mode(ctx);
        let outer_nulls = self.u.set_null_mode(nulls);
        let outer_nesting = self.constraint.set_nesting(self.levels.nesting(ctx.level));
        let vars = self.constraint.count();
        let top_only = crate::ctx::Mode::APPLY_QUALIFIER.with(crate::ctx::Mode::ASSIGNMENT_LHS);
        let inner = ctx.with_mode(ctx.mode.without(top_only));
        let (tree, ahead) = match self.contextual_function(id, pt, inner) {
            Some(t) => (t, false),
            None => match self.typed_ahead_tree(id, inner) {
                Some(t) => (t, true),
                None => (
                    self.splice_tree(id, inner).unwrap_or_else(|| self.typed_unadapted(id, pt, inner)),
                    false,
                ),
            },
        };
        let tree = match self.ast.tree(id) {
            syntax::ast::Tree::Ident { .. } | syntax::ast::Tree::Select { .. }
                if self.explicit_nulls && !ctx.mode.is(crate::ctx::Mode::ASSIGNMENT_LHS) =>
            {
                let select = matches!(self.ast.tree(id), syntax::ast::Tree::Select { .. });
                self.to_not_null_ref(tree, select, inner)
            }
            _ => tree,
        };
        if ahead {
            self.zonk_type(tree);
        } else {
            if let Err(what) = self.interpolate(tree, pt, vars) {
                self.unsupported(id, what);
            }
            self.settle_type(tree, inner);
        }
        let tree = self.structural_call(tree, inner);
        let locked = std::mem::replace(&mut self.locked_vars, vars);
        let adapted = self.adapt(tree, pt, ctx);
        let adapted = self.structural_call(adapted, inner);
        self.locked_vars = locked;
        if let Err(what) = self.reinterpolate((tree, adapted), pt, vars) {
            self.unsupported(id, what);
        }
        self.u.set_comparison_owner(outer);
        self.u.set_null_mode(outer_nulls);
        self.constraint.set_nesting(outer_nesting);
        self.depth -= 1;
        adapted
    }
}
