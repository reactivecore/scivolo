// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//   compiler/src/dotty/tools/dotc/transform/Inlining.scala
//! dotty's `inlining` phase: each call of an inline method that is not itself in an inline method,
//! replaced by an `Inlined` tree of the method's body with the arguments bound (`Inlining`,
//! `Inlines.inlineCall`).

mod beta;
mod blocks;
mod code_of;
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
mod macro_annotations;
mod macro_definitions;
mod macro_impls;
mod macro_trees;
mod macros;
mod no_inits;
mod opaque_proxies;
pub mod outer_this;
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
mod reflect_defs;
mod reflect_lambdas;
mod reflect_trees;
mod reflect_vals;
mod reselect;
mod retype;
mod retyped;
mod skolems;
mod splice;
mod this_proxy;
mod transparent;
mod tuple_intrinsics;
mod type_eval;
mod unapply_fun;
mod unused;

pub use erased::is_erased_value;
pub use host::{Host, Lent};
pub use macros::{AnnotationCall, Annotations, Expanded, MacroArg, MacroCall, MacroError, Macros, Quoted};
pub use quote_match::bound_types;
pub use quote_matcher::quote_match;
pub use reflect::Reflect;

use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tpd, Tree as T, map_children};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use tasty::tags::{INLINE, TRANSPARENT};
use tasty::{Kind, SymbolId, Type, Universe};

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
    /// The implicit values and methods block statements of the code being expanded define, in
    /// whose scope it is, by block, outermost first.
    pub block_implicits: Vec<Vec<SymbolId>>,
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
            block_implicits: Vec::new(),
            case_givens: Vec::new(),
            in_macro_result: false,
        }
    }
}

impl Inlining<'_> {
    /// Expands the inline calls of unit `root` in place.
    pub fn run(&mut self, root: TNodeId) -> Result<(), Unsupported> {
        self.collect_bodies(root);
        let owner = self.u.root();
        let new_root = self.transform(root, owner)?;
        debug_assert_eq!(new_root, root);
        Ok(())
    }

    fn collect_bodies(&mut self, t: TNodeId) {
        if let T::DefDef { rhs: Some(_), .. } = self.tpd.tree(t)
            && let Some(s) = self.tpd.sym(t)
            && self.u.symbol(s).flags.has(INLINE)
        {
            self.bodies.insert(s, t);
        }
        self.note_inits(t);
        for c in self.tpd.children(t) {
            self.collect_bodies(c);
        }
    }

    /// `t`, owned by `owner`, with the inline calls in it expanded; inline definitions stay.
    fn transform(&mut self, t: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let inner = match self.tpd.tree(t) {
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Template { .. } => {
                match self.tpd.sym(t) {
                    Some(s) if self.u.symbol(s).flags.has(INLINE) => return Ok(t),
                    Some(s) => s,
                    None => owner,
                }
            }
            _ => owner,
        };
        self.expand_macro_annotations(t)?;
        let typer_ctx = self.typer_inlined_ctx(t);
        let call = self.traced_call(t);
        let (outer, outer_ctx) = (self.typer_inlined, self.typer_ctx);
        if call.is_some() {
            self.typer_inlined = outer.or(Some(self.tpd.span(t)));
            self.typer_ctx = if outer.is_some() { outer_ctx } else { typer_ctx };
        }
        let mut failed = None;
        let changed = map_children(self, t, &mut |i, c| match Some(c) == call {
            true => c,
            false => i.transform(c, inner).unwrap_or_else(|e| {
                failed.get_or_insert(e);
                c
            }),
        });
        (self.typer_inlined, self.typer_ctx) = (outer, outer_ctx);
        if let Some(e) = failed {
            return Err(e);
        }
        self.fold_constants(t);
        if changed {
            self.retype_inlined(t);
        }
        if let T::UnApply { fun, implicits, patterns } = *self.tpd.tree(t) {
            let fun = self.inlined_unapply_fun(fun, owner)?;
            self.tpd.get_mut(t).tree = T::UnApply { fun, implicits, patterns };
        }
        match self.inline_method(t)? {
            Some(def) => self.inline_call(t, def, owner),
            None => Ok(t),
        }
    }

    /// The definition of the inline method `t` calls with all its arguments, if it is one
    /// (dotty's `needsInlining`).
    fn inline_method(&mut self, t: TNodeId) -> Result<Option<TNodeId>, Unsupported> {
        let Some(tpe) = self.tpd.get(t).tpe else { return Ok(None) };
        let widened = self.u.widen(tpe);
        if self.tpd.is_type_tree(t)
            || matches!(self.u.types().get(widened), Type::Method { .. } | Type::Poly { .. })
        {
            return Ok(None);
        }
        let Some(meth) = self.method_part(t).and_then(|f| self.tpd.sym(f)) else { return Ok(None) };
        let sym = self.u.symbol(meth);
        if sym.kind != Kind::Def
            || !sym.flags.has(INLINE)
            || self.transparent_only && !self.force_inline && !sym.flags.has(TRANSPARENT)
        {
            return Ok(None);
        }
        if let Some(&def) = self.bodies.get(&meth) {
            return Ok(Some(def));
        }
        if self.u.is_deferred(meth) {
            return Err((self.tpd.span(t), "call of a deferred inline method"));
        }
        if self.u.symbol(meth).tree.is_none() {
            return Err((self.tpd.span(t), "call of an inline method defined elsewhere"));
        }
        let def = crate::unpickle::inline_def(self.u, self.tpd, self.names, meth)
            .map_err(|what| (self.tpd.span(t), what))?;
        self.bodies.insert(meth, def);
        Ok(Some(def))
    }

    /// The reference `t` applies, through its applications and type applications.
    pub fn method_part(&self, t: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(t) {
            T::Apply { fun, .. } | T::TypeApply { fun, .. } => self.method_part(fun),
            T::Ident { .. } | T::Select { .. } => Some(t),
            _ => None,
        }
    }

    /// Gives the qualifier of each `this` in `t` its class's symbol and type, as dotty's copy of
    /// the call in `inlineCall` does after the typer.
    pub fn type_this_qualifiers(&mut self, t: TNodeId) {
        if self.transparent_only {
            return;
        }
        if let T::This { qual } = *self.tpd.tree(t)
            && let Some(c) = self.tpd.sym(t)
            && self.tpd.get(qual).tpe.is_none()
        {
            let tpe = crate::defn::type_ref(self.u, c);
            let node = self.tpd.get_mut(qual);
            (node.sym, node.tpe) = (Some(c), Some(tpe));
        }
        for c in self.tpd.traversed(t) {
            self.type_this_qualifiers(c);
        }
    }
}
