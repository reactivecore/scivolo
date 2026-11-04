// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//   compiler/src/dotty/tools/dotc/transform/Inlining.scala
//! dotty's `inlining` phase: each call of an inline method that is not itself in an inline method,
//! replaced by an `Inlined` tree of the method's body with the arguments bound (`Inlining`,
//! `Inlines.inlineCall`).

mod beta;
mod blocks;
mod code_of;
mod code_of_refs;
mod compiletime;
mod const_fold;
mod constants;
mod copy;
mod copy_classes;
mod erased;
mod expand;
mod host;
mod implicit_args;
mod inline_call;
mod local_implicits;
mod macro_annotations;
mod macro_args;
mod macro_definitions;
mod macro_impls;
mod macro_trees;
mod macros;
mod no_inits;
mod opaque_proxies;
mod opaque_results;
pub mod outer_this;
mod phase;
mod prefix_seen;
mod proxies;
mod purity;
mod quote_match;
mod quote_matcher;
mod quoted;
mod reduce;
mod reflect;
mod reflect_applies;
mod reflect_beta;
mod reflect_classes;
mod reflect_constructors;
mod reflect_defs;
mod reflect_lambdas;
mod reflect_methods;
mod reflect_trees;
mod reflect_vals;
mod reselect;
mod retype;
mod retyped;
mod skolems;
mod splice;
mod spliced_types;
mod static_opaques;
mod this_proxy;
mod transparent;
mod tuple_intrinsics;
mod type_checks;
mod type_eval;
mod unapply_fun;
mod unused;

pub use erased::is_erased_value;
pub use host::{CheckedErrors, Host, ImportSelector, Lent};
pub use local_implicits::{implicit_defs_at, is_implicit_def};
pub use macros::{
    AnnotationCall, Annotations, Expanded, MacroArg, MacroCall, MacroError, Macros, Quoted, Reports, Search,
};
pub use purity::Purity;
pub use quote_match::bound_types;
pub use quote_matcher::quote_match;
pub use reflect::Reflect;

use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tpd};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use tasty::{SymbolId, Universe};

/// How many inline calls may expand one inside another (dotty's `-Xmax-inlines`).
const MAX_INLINES: u32 = 32;

/// Where and why a unit's inlining is outside the subset.
pub type Unsupported = (Span, &'static str);

pub struct Inlining<'a> {
    pub u: &'a mut Universe,
    pub tpd: &'a mut Tpd,
    pub names: &'a mut Names,
    pub fresh: &'a mut FreshNames,
    /// The definition of each inline method of the unit with a body.
    bodies: FxHashMap<SymbolId, TNodeId>,
    /// How many inline calls are being expanded, one inside another.
    depth: u32,
    /// The staging level of the tree being transformed: inside a quote's body, calls stay.
    staging: i32,
    /// The class enclosing the call being expanded.
    pub enclosing_class: Option<SymbolId>,
    /// The span of the call being expanded.
    pub call_span: Span,
    /// The classes of the unit whose templates run code.
    with_inits: FxHashSet<SymbolId>,
    /// Whether only calls of transparent inline methods expand, as in the typer.
    transparent_only: bool,
    /// Whether every inline call expands: in the condition of an inline `if` or the selector of
    /// an inline match (dotty's `Mode.ForceInline`).
    force_inline: bool,
    host: &'a mut dyn Host,
    /// The context of the outermost call being expanded.
    pub ctx: Option<Ctx>,
    /// The span of the outermost call being expanded.
    pub outermost_span: Span,
    /// The span of the outermost `Inlined` tree from the typer around the code being expanded.
    typer_inlined: Option<Span>,
    /// The context of the call of that tree, if the typer recorded one, and the tree.
    typer_ctx: Option<(Ctx, TNodeId)>,
    /// The implicits that tree defines, while a call inside it without a context of its own
    /// expands in its context.
    pub outer_implicits: Vec<SymbolId>,
    /// Whether the code being expanded is in the scope of an implicit it defines other than by a
    /// block statement or a `summonFrom` case.
    pub local_implicits: bool,
    /// The scopes of the implicits block statements of the code being expanded define and of its
    /// import statements, in which it is, outermost first.
    pub scopes: Vec<local_implicits::Scope>,
    /// The bindings of the `summonFrom` cases whose bodies are being expanded, innermost last.
    pub case_givens: Vec<SymbolId>,
    /// Whether the code being retyped is a macro's result, whose references to methods with a
    /// using clause take their implicit arguments.
    pub in_macro_result: bool,
}

impl AsMut<Tpd> for Inlining<'_> {
    fn as_mut(&mut self) -> &mut Tpd {
        self.tpd
    }
}

impl<'a> Inlining<'a> {
    pub fn new(
        (u, tpd): (&'a mut Universe, &'a mut Tpd),
        (names, fresh): (&'a mut Names, &'a mut FreshNames),
        host: &'a mut dyn Host,
        transparent_only: bool,
    ) -> Self {
        Inlining {
            u,
            tpd,
            names,
            fresh,
            bodies: FxHashMap::default(),
            depth: 0,
            staging: 0,
            enclosing_class: None,
            call_span: Span::NONE,
            with_inits: FxHashSet::default(),
            transparent_only,
            force_inline: false,
            host,
            ctx: None,
            outermost_span: Span::NONE,
            typer_inlined: None,
            typer_ctx: None,
            outer_implicits: Vec::new(),
            local_implicits: false,
            scopes: Vec::new(),
            case_givens: Vec::new(),
            in_macro_result: false,
        }
    }
}
