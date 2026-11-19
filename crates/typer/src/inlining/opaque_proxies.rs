// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The proxy of the `this` of an object or class with opaque types around an inline method,
//! bound through a proxy whose refined type exposes the aliases (dotty's `canElideThis`,
//! `accountForOpaques`, `addOpaqueProxies`, `OpaqueProxy`).

use super::expand::Expansion;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::tags::{INLINEPROXY, OPAQUE, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl Expansion<'_, '_> {
    /// The object with opaque types enclosing method `meth` whose `this` body `body` refers to,
    /// with the proxy of that `this`, its bindings put after the `$proxyN` of the arguments:
    /// `$proxyN`, the object cast to its type refined by the aliases, and `C$_this`, that proxy;
    /// for a class whose `this` is among the proxies `this`, that proxy bound to a `$proxyN`; for
    /// a call through the `C$_this` of an enclosing expansion, a `$proxyN` of that `C$_this`.
    pub fn bind_opaque_this(
        &mut self,
        (fun, meth): (TNodeId, SymbolId),
        body: TNodeId,
        this: &[(SymbolId, SymbolId)],
    ) -> Result<Option<(SymbolId, SymbolId)>, Unsupported> {
        let mut owner = self.i.u.symbol(meth).owner;
        let module = loop {
            let Some(o) = owner else { return Ok(None) };
            if self.i.u.has_opaques(o) {
                break o;
            }
            owner = self.i.u.symbol(o).owner;
        };
        let (mut uses, mut outers) = (false, Vec::new());
        self.i.this_refs(body, module, &mut uses, &mut outers);
        if !uses {
            return Ok(None);
        }
        let refused = self.err("inline method of an object with opaque types");
        if self.i.u.sees_opaques(module) {
            return Err(refused);
        }
        if let Some(&(_, proxy)) = this.iter().find(|&&(c, _)| c == module) {
            return self.open_class_this(meth, proxy).map(|()| None);
        }
        if let Some(outer) = self.enclosing_this_proxy(fun) {
            return self.bind_nested_opaque_this(module, meth, outer).map(Some);
        }
        let mut o = Some(self.owner);
        while let Some(x) = o.filter(|&x| x != module) {
            o = self.i.u.symbol(x).owner;
        }
        let inside = o.is_some();
        let source =
            self.i.u.source_module(module).filter(|_| self.i.u.is_static_owner(module) && this.is_empty());
        if inside && self.prefix_is_this_of(fun, meth, module) {
            return Ok(None);
        }
        let Some(source) = source else { return Err(refused) };
        if inside {
            return self.bind_inner_opaque_this(module, source).map(Some);
        }
        let module_ref = crate::defn::type_ref(self.i.u, source);
        let point = Span::at(self.i.tpd.span(self.call).point());
        let at = self.opaque_refs.len();
        let existing =
            self.opaque_refs.iter().find_map(|&(r, p)| (self.i.u.sym_of(r) == Some(source)).then_some(p));
        let opaque = match existing {
            Some(p) => p,
            None => {
                let refined = self.i.opaques_exposed(module_ref, module);
                let name = self.i.fresh_name("$proxy");
                let text = self.i.names.text(name).to_owned();
                let opaque = self.local(text, Flags::default().with(SYNTHETIC), refined);
                let module_tree = self.i.singleton(module_ref, point).ok_or(self.err("object reference"))?;
                let cast = self.i.cast(module_tree, refined)?;
                self.bind_proxy(opaque, cast, point, at);
                opaque
            }
        };
        let opaque_ref = self.i.u.intern_type(Type::Sym(None, opaque));
        let proxy = self.this_proxy_symbol(module, opaque_ref);
        let source_name = self.i.u.symbol(source).name.clone();
        let ident = TName { name: self.i.names.intern(&source_name), is_type: false };
        let rhs = self.i.tpd.node(T::Ident { name: ident }, point, Some(opaque_ref), Some(opaque));
        self.bind_proxy(proxy, rhs, point, at + usize::from(existing.is_none()));
        Ok(Some((module, proxy)))
    }

    /// The type of the `C$_this` of an enclosing expansion that call reference `fun` goes through.
    fn enclosing_this_proxy(&mut self, fun: TNodeId) -> Option<TypeId> {
        let q = self.call_prefix(fun)?;
        let qt = self.i.tpd.tpe(q);
        let s = self.i.u.sym_of(qt)?;
        self.i.u.symbol(s).flags.has(INLINEPROXY).then_some(qt)
    }

    /// The proxy of the `this` of `module` for a call through `outer`, an enclosing `C$_this`:
    /// `C$_this`, bound to a `$proxyN` of `outer` that exposes the aliases.
    fn bind_nested_opaque_this(
        &mut self,
        module: SymbolId,
        meth: SymbolId,
        outer: TypeId,
    ) -> Result<(SymbolId, SymbolId), Unsupported> {
        let point = Span::at(self.i.tpd.span(self.call).point());
        let opaque = self
            .opaque_proxy(outer, Some(meth), point)
            .ok_or(self.err("inline method of an object with opaque types"))?;
        let opaque_ref = self.i.u.intern_type(Type::Sym(None, opaque));
        let proxy = self.this_proxy_symbol(module, opaque_ref);
        let outer_sym = self.i.u.sym_of(outer).expect("a reference to a proxy");
        let outer_name = self.i.u.symbol(outer_sym).name.clone();
        let name = TName { name: self.i.names.intern(&outer_name), is_type: false };
        let rhs = self.i.tpd.node(T::Ident { name }, point, Some(opaque_ref), Some(opaque));
        self.bind_proxy(proxy, rhs, point, self.opaque_refs.len());
        Ok((module, proxy))
    }

    /// Rebinds `proxy`, the proxy of the `this` of a class with opaque types bound to a path, to
    /// the `$proxyN` of that path (dotty's `accountForOpaques`).
    fn open_class_this(&mut self, meth: SymbolId, proxy: SymbolId) -> Result<(), Unsupported> {
        let refused = self.err("inline method of a class with opaque types through a prefix that is no path");
        let path = self.i.u.info(proxy);
        if !self.i.u.sym_of(path).is_some_and(|s| self.i.u.symbol(s).kind.is_term()) {
            return Err(refused);
        }
        let call_span = self.i.tpd.span(self.call);
        let opaque = self.opaque_proxy(path, Some(meth), call_span).ok_or(refused)?;
        let at = self.bindings.iter().position(|&b| self.i.tpd.sym(b) == Some(proxy)).ok_or(refused)?;
        let T::ValDef { tpt, rhs: Some(rhs), .. } = *self.i.tpd.tree(self.bindings[at]) else {
            return Err(refused);
        };
        let T::Ident { name } = *self.i.tpd.tree(rhs) else { return Err(refused) };
        let opaque_ref = self.i.u.intern_type(Type::Sym(None, opaque));
        self.i.u.set_info(proxy, opaque_ref);
        let (rhs_span, tpt_span) = (self.i.tpd.span(rhs), self.i.tpd.span(tpt));
        let rhs = self.i.tpd.node(T::Ident { name }, rhs_span, Some(opaque_ref), Some(opaque));
        self.bindings.remove(at);
        let defined = self.bindings.iter().position(|&b| self.i.tpd.sym(b) == Some(opaque)).ok_or(refused)?;
        self.bind_proxy(proxy, rhs, tpt_span, at.max(defined + 1));
        Ok(())
    }

    pub fn local(&mut self, name: String, flags: Flags, info: TypeId) -> SymbolId {
        let s = self.i.u.define(Symbol {
            name,
            kind: Kind::Val,
            owner: Some(self.owner),
            flags,
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.i.u.set_info(s, info);
        s
    }
}

impl Inlining<'_> {
    /// `module_ref` refined by the aliases of the opaque types of its class `module`, the last
    /// declared innermost (dotty's `OpaqueProxy.apply`).
    pub fn opaques_exposed(&mut self, module_ref: TypeId, module: SymbolId) -> TypeId {
        let decls = self.u.symbol(module).decls.clone();
        let mut refined = module_ref;
        for d in decls.into_iter().rev() {
            let s = self.u.symbol(d);
            if s.kind != Kind::Type || !s.flags.has(OPAQUE) {
                continue;
            }
            let name = self.u.interned_name(d);
            let Some(alias) = self.u.opaque_rhs(d) else { continue };
            if let Some(binder) = self.u.lambda(alias).map(|l| l.binder) {
                let variances = self.u.variances(d);
                self.u.declare_lambda_variances(binder, variances);
            }
            let seen = self.u.as_seen_from(alias, module_ref, module);
            let info = self.u.intern_type(Type::Alias(seen));
            refined = self.u.intern_type(Type::Refined(refined, name, info));
        }
        refined
    }
}
