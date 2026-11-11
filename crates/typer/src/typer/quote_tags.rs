// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/staging/HealType.scala
//! The given `Type[T]` of each type `T` of level 0 a quote refers to (dotty's `tryHeal`), which
//! the lowering's `PickleQuotes` makes the quote's tags.

use super::Typer;
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashSet;
use syntax::ast::flags::{CASE, GIVEN};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Records the tags of quote `quote` of body `body`, typed at level 0 in `ctx`: for each abstract
    /// type not static and defined outside the quote that a type in the body outside its splices
    /// refers to, the stable path a search for its `Type[T]` finds.
    pub fn record_quote_tags(&mut self, quote: TNodeId, body: TNodeId, ctx: Ctx) {
        let (mut todo, mut defined, mut used) = (vec![body], FxHashSet::default(), Vec::new());
        while let Some(t) = todo.pop() {
            let node = self.tpd.get(t);
            if matches!(node.tree, T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. }) {
                defined.extend(node.sym);
            }
            if let Some(tpe) = node.tpe {
                let tpe = self.zonk(tpe);
                self.u.map_type(tpe, &mut |u, x| {
                    if let Some(s) = this_or_local(u, x)
                        && !used.iter().any(|&(u, _)| u == s)
                    {
                        used.push((s, x));
                    }
                    None
                });
            }
            if !self.is_splice(t) {
                todo.extend(self.tpd.children(t).iter().rev());
            }
        }
        let Some(class) = self.resolve_full_name("scala.quoted.Type", true) else { return };
        let mut tags = Vec::new();
        for (s, arg) in used {
            let mut owners = std::iter::successors(Some(s), |&o| self.u.symbol(o).owner);
            if owners.any(|o| defined.contains(&o)) || !self.is_level_zero_type(s) {
                continue;
            }
            let tycon = type_ref(self.u, class);
            let pt = self.u.intern_applied(tycon, &[arg]);
            let found = self.implicit_arg(pt, self.tpd.span(body), ctx, 0).map(|tag| self.tpd.tpe(tag));
            match (found, self.tpd.pattern_given(s)) {
                (Ok(path), _) if self.u.sym_of(path).is_some_and(|t| self.u.symbol(t).kind.is_term()) => {
                    tags.push((s, path))
                }
                (_, Some(given)) => tags.push((s, self.u.intern_type(Type::Sym(None, given)))),
                _ => {}
            }
        }
        if !tags.is_empty() {
            self.tpd.set_quote_tags(quote, tags);
        }
    }

    /// Records the tags of quote pattern `pattern` of body `body` at level 0 in `ctx`, and for
    /// each of its type variables `bindings` the given `t$givenN: Type[t]` its encoding binds
    /// (dotty's `QuotePatterns.encode`).
    pub fn record_pattern_givens(&mut self, pattern: TNodeId, body: TNodeId, bindings: &[TNodeId], ctx: Ctx) {
        self.record_quote_tags(pattern, body, ctx);
        let Some(class) = self.resolve_full_name("scala.quoted.Type", true) else { return };
        for &b in bindings {
            let Some(t) = self.tpd.sym(b) else { continue };
            let name = format!("{}$given", self.u.symbol(t).name);
            let name = self.fresh.next(self.names, &name);
            let name = self.names.text(name).to_owned();
            let given = self.syms.define(self.u, &name, Kind::Val, ctx.owner, GIVEN.or(CASE));
            let arg = self.u.intern_type(Type::Sym(None, t));
            let tycon = type_ref(self.u, class);
            let info = self.u.intern_applied(tycon, &[arg]);
            self.u.set_info(given, info);
            self.tpd.set_pattern_given(t, given);
        }
    }

    /// `applied`, a type application of method `fun`, with its tags recorded if it is a type
    /// quote `scala.quoted.Type.of[T]` at level 0 in `ctx`.
    pub fn with_type_quote_tags(&mut self, applied: TNodeId, fun: Option<&str>, ctx: Ctx) -> TNodeId {
        if ctx.staging == 0
            && fun == Some("scala.quoted.Type$.of")
            && let T::TypeApply { args, .. } = *self.tpd.tree(applied)
            && let &[body] = self.tpd.items(args)
        {
            self.record_quote_tags(applied, body, ctx);
        }
        applied
    }

    /// Records the tags of implicit argument `tree` in `ctx` if it is a type quote
    /// `Type.of[T](quotes)`.
    pub fn record_implicit_type_quote(&mut self, tree: TNodeId, ctx: Ctx) {
        let T::Apply { fun, .. } = *self.tpd.tree(tree) else { return };
        let T::TypeApply { fun: of, .. } = *self.tpd.tree(fun) else { return };
        let name = self.tpd.sym(of).map(|s| self.u.full_name(s));
        self.with_type_quote_tags(fun, name.as_deref(), ctx);
    }

    /// The body of `quote(body).apply(quotes)`.
    pub fn quote_body(&self, applied: TNodeId) -> Option<TNodeId> {
        let T::Apply { fun, .. } = *self.tpd.tree(applied) else { return None };
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return None };
        let T::Apply { args, .. } = *self.tpd.tree(qualifier) else { return None };
        self.tpd.items(args).first().copied()
    }

    /// Whether `t` calls `scala.quoted.runtime.Expr.splice`.
    pub fn is_splice(&self, t: TNodeId) -> bool {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return false };
        let m = self.tpd.meth_part(fun);
        self.tpd.sym(m).is_some_and(|s| self.is_staging_method(s) && self.u.symbol(s).name == "splice")
    }

    /// Whether `s` is an abstract type that is not static.
    fn is_level_zero_type(&mut self, s: SymbolId) -> bool {
        matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam)
            && !self.u.is_static_symbol(s)
            && self.u.alias_of(s).is_none()
    }
}

/// The symbol `t` refers to without a prefix or through its owner's `this`.
pub fn this_or_local(u: &Universe, t: TypeId) -> Option<SymbolId> {
    match u.sym_ref(t) {
        Some((None, s)) => Some(s),
        Some((Some(p), s)) if u.this_of(p).is_some() => Some(s),
        _ => None,
    }
}
