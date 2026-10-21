// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The proxy of the `this` of an inline method's class: bound to the call's prefix unless the
//! call is inside the class, and the trees standing for `this` in the expansion (dotty's
//! `registerType` for `this`, `computeThisBindings`, `canElideThis`).

use super::expand::Expansion;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Expansion<'_, '_> {
    /// The classes and proxies of the `this`es the call needs, innermost first, their bindings
    /// put first: none if the method is a static object's or the call is inside its class, nor
    /// for its class if the body does not refer to its `this` and the prefix is idempotent.
    pub fn bind_this(
        &mut self,
        fun: TNodeId,
        meth: SymbolId,
        body: TNodeId,
    ) -> Result<Vec<(SymbolId, SymbolId)>, Unsupported> {
        let cls = self.i.u.symbol(meth).owner.ok_or(self.err("inline method"))?;
        let prefix = match *self.i.tpd.tree(fun) {
            T::Select { qualifier, .. } => Some(qualifier),
            _ => None,
        };
        let impure = prefix.is_some_and(|q| !self.i.is_idempotent_expr(q));
        if self.i.is_static_owner(cls) {
            return if impure { Err(self.err("inline call of an impure prefix")) } else { Ok(Vec::new()) };
        }
        if self.i.u.symbol(cls).kind.is_term() {
            return Ok(Vec::new());
        }
        if self.i.u.symbol(cls).kind != Kind::Class {
            return Err(self.err("inline method of a type"));
        }
        if self.i.u.is_module_class(cls) {
            return Err(self.err("inline method of a nested object"));
        }
        if !self.i.u.type_params(cls).is_empty() {
            return Err(self.err("inline method of a generic class"));
        }
        let mut uses = false;
        let mut outers = Vec::new();
        self.i.this_refs(body, cls, &mut uses, &mut outers);
        let outer_err = self.err("inline body referring to an outer this");
        let Some(q) = prefix else { return if outers.is_empty() { Ok(Vec::new()) } else { Err(outer_err) } };
        if matches!(self.i.tpd.tree(q), T::Super { .. }) {
            return match (uses, outers.is_empty()) {
                (false, true) => Ok(Vec::new()),
                (true, _) => Err(self.err("inline call through super")),
                (false, false) => Err(outer_err),
            };
        }
        let qt = self.i.tpd.tpe(q);
        let qtv = self.i.u.types().get(qt).clone();
        if matches!(qtv, Type::This(c) if self.i.u.class_symbol(c) == Some(cls)) {
            return if outers.is_empty() { Ok(Vec::new()) } else { Err(outer_err) };
        }
        let mut proxies = Vec::new();
        if uses || impure {
            proxies.push((cls, self.bind_class_this(cls, q, qt)?));
        }
        let pure = !proxies.is_empty() || self.i.is_pure_expr(q);
        let at = proxies.len();
        proxies.extend(self.bind_outer_this(cls, qt, &outers, (pure, at))?);
        Ok(proxies)
    }

    /// The proxy of the `this` of class `cls` for call prefix `q` of type `qt`, bound first.
    fn bind_class_this(&mut self, cls: SymbolId, q: TNodeId, qt: TypeId) -> Result<SymbolId, Unsupported> {
        let stable = match *self.i.u.types().get(qt) {
            Type::Sym(_, s) => self.i.u.symbol(s).kind.is_term() && self.i.is_stable_member(s),
            Type::This(_) => true,
            _ => false,
        };
        let info = if stable { qt } else { self.i.u.widen(qt) };
        let proxy = self.this_proxy_symbol(cls, info);
        let call_span = self.i.tpd.span(self.call);
        let point = Span::at(call_span.point());
        let widened = self.i.u.widen(info);
        let widened_cls = self.i.u.class_symbol(widened);
        let module = widened_cls.filter(|&c| self.i.u.is_module_class(c) && self.i.is_static_owner(c));
        let module = module.and_then(|c| self.i.u.source_module(c));
        let term_ref = matches!(self.i.u.types().get(qt), Type::Sym(..));
        let pure_ref = if stable && term_ref && self.i.is_pure_expr(q) {
            self.i.singleton(qt, point)
        } else if let Some(m) = module {
            let tpe = crate::defn::type_ref(self.i.u, m);
            self.i.singleton(tpe, point)
        } else {
            None
        };
        let (rhs, tpt_span) = match pure_ref {
            Some(r) => (r, point),
            None => {
                let mut m = super::copy::TreeTypeMap {
                    old_owner: Some(self.owner),
                    new_owner: Some(proxy),
                    ..Default::default()
                };
                let rhs = self.i.copy(&mut m, q, &mut |_, _, _| None)?;
                (rhs, Span::at(self.i.tpd.span(rhs).start()))
            }
        };
        self.bind_proxy(proxy, rhs, tpt_span, 0);
        Ok(proxy)
    }
}

impl Inlining<'_> {
    /// Identifier `t` as a selection from the `this` of its prefix if the prefix cannot be elided
    /// where the expansion is (dotty's `needsSelect` in `InlineTyper.typedIdent`).
    pub fn ident_as_select(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        let (&T::Ident { name }, Some(tpe)) = (self.tpd.tree(t), self.tpd.get(t).tpe) else { return Ok(t) };
        let Type::Sym(Some(p), s) = *self.u.types().get(tpe) else { return Ok(t) };
        if name.is_type || self.is_elidable_prefix(p, s) {
            return Ok(t);
        }
        let span = self.tpd.span(t);
        let this =
            self.this_tree(p, span).ok_or((span, "inline body identifier of a prefix other than this"))?;
        Ok(self.tpd.node(T::Select { qualifier: this, name }, span, Some(tpe), Some(s)))
    }

    /// Notes in `uses` whether the leaves of `t` refer to the `this` of class `cls`, and adds to
    /// `outer` the other classes that are no static owners whose `this` they refer to.
    fn this_refs(&mut self, t: TNodeId, cls: SymbolId, uses: &mut bool, outer: &mut Vec<SymbolId>) {
        let leaf = matches!(self.tpd.tree(t), T::This { .. } | T::Ident { .. } | T::TypeTree { .. });
        if leaf && let Some(tpe) = self.tpd.get(t).tpe {
            let mut classes = Vec::new();
            self.u.map_type(tpe, &mut |u, part| {
                if let Type::This(c) = *u.types().get(part) {
                    classes.extend(u.class_symbol(c));
                }
                None
            });
            for c in classes {
                if c == cls {
                    *uses = true;
                } else if !self.is_static_owner(c) && !outer.contains(&c) {
                    outer.push(c);
                }
            }
        }
        for c in self.tpd.children(t) {
            self.this_refs(c, cls, uses, outer);
        }
    }
}

/// The tree replacing `t` in the copied body if it refers to the `this` of class `cls` proxied by
/// `proxy`: the proxy as an argument inlined from outside for `this`, a selection from the proxy
/// for an identifier of a member.
pub fn replace_this(
    i: &mut Inlining<'_>,
    cls: SymbolId,
    proxy: SymbolId,
    call: Span,
    t: TNodeId,
) -> Option<TNodeId> {
    let tpe = i.tpd.get(t).tpe?;
    let proxy_tpe = i.u.intern_type(Type::Sym(None, proxy));
    let is_cls = |i: &mut Inlining<'_>, c| i.u.class_symbol(c) == Some(cls);
    let prefix_this = match *i.u.types().get(tpe) {
        Type::Sym(Some(p), _) => match *i.u.types().get(p) {
            Type::This(c) => Some(c),
            _ => None,
        },
        _ => None,
    };
    let prefix_is_cls = prefix_this.is_some_and(|c| is_cls(i, c));
    match (i.tpd.tree(t).clone(), i.u.types().get(tpe).clone()) {
        (T::This { .. }, Type::This(c)) if is_cls(i, c) => {
            let inner = i.singleton(proxy_tpe, call)?;
            let empty = i.tpd.list(&[]);
            let span = i.tpd.span(t);
            Some(i.tpd.node(
                T::Inlined { call: None, bindings: empty, expansion: inner },
                span,
                Some(proxy_tpe),
                None,
            ))
        }
        (T::Ident { name }, Type::Sym(Some(_), s)) if !name.is_type && prefix_is_cls => {
            let span = i.tpd.span(t);
            let qualifier = i.singleton(proxy_tpe, Span::at(span.start()))?;
            let tpe = i.u.intern_type(Type::Sym(Some(proxy_tpe), s));
            Some(i.tpd.node(T::Select { qualifier, name }, span, Some(tpe), Some(s)))
        }
        _ => None,
    }
}
