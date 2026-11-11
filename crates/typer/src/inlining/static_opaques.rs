// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The proxies `$proxyN` of the objects with opaque types that an inline call's arguments or
//! prefix refer to, and the proxy of the `this` of a static object with opaque types read from
//! TASTy bound to one (dotty's `mapOpaquesInValueArg`, `addOpaqueProxies`,
//! `accountForOpaques`, `canElideThis` on `seesOpaques`).

use super::Unsupported;
use super::expand::Expansion;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::tags::{OPAQUE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};

impl Expansion<'_, '_> {
    /// Binds a `$proxyN` to each object with opaque types enclosing method `meth` that the
    /// widened types of the call's value arguments refer to, in order, and records the type
    /// each argument is cast to: its type and its widened type with the proxies in place.
    pub fn bind_arg_opaques(&mut self, meth: SymbolId) {
        for arg in self.value_args() {
            let tpe = self.i.tpd.tpe(arg);
            let widened = self.i.u.widen(tpe);
            let dealiased = self.i.u.dealias(widened);
            let mut refs = Vec::new();
            self.i.u.map_type(dealiased, &mut |u, part| {
                if let Some(s) = u.sym_of(part)
                    && u.symbol(s).kind.is_term()
                    && !refs.contains(&part)
                {
                    refs.push(part);
                }
                None
            });
            let span = self.i.tpd.span(arg);
            for r in refs {
                self.opaque_proxy(r, Some(meth), span);
            }
            if self.opaque_refs.is_empty() {
                continue;
            }
            let proxies = self.opaque_refs.clone();
            let mapped = self.i.u.map_type(dealiased, &mut |u, part| {
                let (_, p) = proxies.iter().find(|&&(r, _)| r == part)?;
                Some(u.intern_type(Type::Sym(None, *p)))
            });
            if mapped != dealiased {
                let both = self.i.u.intern_type(Type::And(tpe, mapped));
                self.opaque_args.push((arg, both));
            }
        }
    }

    /// The proxy of the `this` of static object class `cls` with opaque types, whose member
    /// `meth` the call reaches through prefix `prefix` and whose `this` body `body` refers to,
    /// bound to the `$proxyN` of the prefix; none if the call is inside `cls`; refused for an
    /// `impure` prefix.
    pub fn bind_static_opaque_this(
        &mut self,
        (cls, meth): (SymbolId, SymbolId),
        body: TNodeId,
        (prefix, impure): (Option<TNodeId>, bool),
    ) -> Result<Vec<(SymbolId, SymbolId)>, Unsupported> {
        if impure {
            return Err(self.err("inline call of an impure prefix"));
        }
        let Some(q) = prefix else { return Ok(Vec::new()) };
        let qt = self.i.tpd.tpe(q);
        let this_prefix = self.i.u.this_of(qt).is_some();
        if this_prefix || !self.sees_tasty_opaques(cls) || !encloses(self, cls, meth) {
            return Ok(Vec::new());
        }
        let (mut uses, mut outers) = (false, Vec::new());
        self.i.this_refs(body, cls, &mut uses, &mut outers);
        if !uses {
            return Ok(Vec::new());
        }
        let refused = self.err("inline call of an object with opaque types through a prefix that is no path");
        if !self.i.is_singleton_path(qt) {
            return Err(refused);
        }
        let call_span = self.i.tpd.span(self.call);
        let opaque = self.opaque_proxy(qt, None, call_span).ok_or(refused)?;
        let opaque_ref = self.i.u.intern_type(Type::Sym(None, opaque));
        let proxy = self.this_proxy_symbol(cls, opaque_ref);
        let point = Span::at(self.i.tpd.span(self.call).point());
        let source = self.i.u.source_module(cls).ok_or(refused)?;
        let name = self.i.u.symbol(source).name.clone();
        let ident = TName { name: self.i.names.intern(&name), is_type: false };
        let rhs = self.i.tpd.node(T::Ident { name: ident }, point, Some(opaque_ref), Some(opaque));
        self.bind_proxy(proxy, rhs, point, self.opaque_refs.len());
        Ok(vec![(cls, proxy)])
    }

    /// The `$proxyN` of path `r` if it refers to an object with opaque types, one enclosing
    /// `within` if given, bound after the others at `span`.
    pub fn opaque_proxy(&mut self, r: TypeId, within: Option<SymbolId>, span: Span) -> Option<SymbolId> {
        if let Some(&(_, p)) = self.opaque_refs.iter().find(|&&(x, _)| x == r) {
            return Some(p);
        }
        let mut widened = self.i.u.widen(r);
        while let Type::Refined(parent, ..) = *self.i.u.ty(widened) {
            widened = self.i.u.widen(parent);
        }
        let class = self.i.u.class_symbol(widened)?;
        let bases = self.i.u.linearization(class);
        let cls = bases.into_iter().find(|&c| {
            let opaques = self.contains_tasty_opaques(c) || self.i.u.has_opaques(c);
            opaques && within.is_none_or(|m| encloses(self, c, m))
        })?;
        let point = Span::at(span.start());
        let refined = self.i.opaques_exposed(r, cls);
        let rhs = self.i.singleton(r, point)?;
        let rhs = self.i.cast(rhs, refined).ok()?;
        let name = self.i.fresh.next(self.i.names, "$proxy");
        let text = self.i.names.text(name).to_owned();
        let proxy = self.local(text, Flags::default().with(SYNTHETIC), refined);
        let at = self.opaque_refs.len();
        self.bind_proxy(proxy, rhs, point, at);
        self.i.tpd.get_mut(self.bindings[at]).span = self.i.tpd.span(self.call);
        self.opaque_refs.push((r, proxy));
        Some(proxy)
    }

    /// Whether class `c` declares opaque types, read from TASTy (dotty's `containsOpaques`).
    fn contains_tasty_opaques(&self, c: SymbolId) -> bool {
        let s = self.i.u.symbol(c);
        s.kind == Kind::Class && s.flags.has(OPAQUE) && !self.i.u.has_opaques(c)
    }

    /// Whether class `c` or an object class enclosing it declares opaque types read from TASTy
    /// (dotty's `seesOpaques`).
    fn sees_tasty_opaques(&self, c: SymbolId) -> bool {
        let mut o = Some(c);
        while let Some(x) = o.filter(|&x| self.i.u.symbol(x).kind == Kind::Class) {
            if self.contains_tasty_opaques(x) {
                return true;
            }
            if !self.i.u.is_module_class(x) {
                return false;
            }
            o = self.i.u.symbol(x).owner;
        }
        false
    }

    /// The value arguments of the call, clause by clause.
    fn value_args(&self) -> Vec<TNodeId> {
        let (mut t, mut clauses) = (self.call, Vec::new());
        loop {
            match *self.i.tpd.tree(t) {
                T::Apply { fun, args } => {
                    clauses.push(self.i.tpd.items(args).to_vec());
                    t = fun;
                }
                T::TypeApply { fun, .. } => t = fun,
                _ => break,
            }
        }
        let unnamed = |a: TNodeId| match *self.i.tpd.tree(a) {
            T::NamedArg { arg, .. } => arg,
            _ => a,
        };
        clauses.into_iter().rev().flatten().map(unnamed).collect()
    }
}

/// Whether `inner` is `outer` or owned by it, transitively.
fn encloses(e: &Expansion<'_, '_>, outer: SymbolId, inner: SymbolId) -> bool {
    std::iter::successors(Some(inner), |&s| e.i.u.symbol(s).owner).any(|s| s == outer)
}
