// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The proxy of the `this` of a static object with opaque types around an inline method, bound
//! through a proxy whose refined type exposes the aliases (dotty's `canElideThis`,
//! `accountForOpaques`, `addOpaqueProxies`, `OpaqueProxy`).

use super::expand::Expansion;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::tags::{OPAQUE, SYNTHETIC, TRANSPARENT};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl Expansion<'_, '_> {
    /// The object with opaque types enclosing method `meth` whose `this` body `body` refers to,
    /// with the proxy of that `this`, its bindings put first: `$proxyN`, the object cast to its
    /// type refined by the aliases, and `C$_this`, that proxy.
    pub fn bind_opaque_this(
        &mut self,
        meth: SymbolId,
        body: TNodeId,
        others: usize,
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
        let info = self.i.u.info(meth);
        let in_signature = self.i.mentions_this(info, module);
        if !uses && !in_signature {
            return Ok(None);
        }
        if !uses || self.i.u.sees_opaques(module) {
            return Err(self.err("inline method of an object with opaque types"));
        }
        let mut o = Some(self.owner);
        while let Some(x) = o.filter(|&x| x != module) {
            o = self.i.u.symbol(x).owner;
        }
        let inside = o.is_some();
        let source = self.i.u.source_module(module).filter(|_| self.i.is_static_owner(module) && !inside);
        let Some(source) = source.filter(|&s| others == 0 && !self.args_mention(s)) else {
            return Err(self.err("inline method of an object with opaque types"));
        };
        let module_ref = crate::defn::type_ref(self.i.u, source);
        let refined = self.i.opaques_exposed(module_ref, module);
        let point = Span::at(self.i.tpd.span(self.call).point());
        let name = self.i.fresh.next(self.i.names, "$proxy");
        let text = self.i.names.text(name).to_owned();
        let opaque = self.local(text, Flags::default().with(SYNTHETIC), refined);
        let module_tree = self.i.singleton(module_ref, point).ok_or(self.err("object reference"))?;
        let cast = self.i.cast(module_tree, refined)?;
        self.bind_proxy(opaque, cast, point, 0);
        let opaque_ref = self.i.u.intern_type(Type::Sym(None, opaque));
        let proxy = self.this_proxy_symbol(module, opaque_ref);
        let source_name = self.i.u.symbol(source).name.clone();
        let ident = TName { name: self.i.names.intern(&source_name), is_type: false };
        let rhs = self.i.tpd.node(T::Ident { name: ident }, point, Some(opaque_ref), Some(opaque));
        self.bind_proxy(proxy, rhs, point, 1);
        Ok(Some((module, proxy)))
    }

    /// Whether the widened type of an argument of the call refers to object `module`, which
    /// dotty's `mapOpaquesInValueArg` would cast.
    fn args_mention(&mut self, module: SymbolId) -> bool {
        let mut t = self.call;
        loop {
            match *self.i.tpd.tree(t) {
                T::Apply { fun, args } => {
                    for a in self.i.tpd.items(args).to_vec() {
                        let tpe = self.i.tpd.tpe(a);
                        let widened = self.i.u.widen(tpe);
                        if self.i.mentions(widened, module) {
                            return true;
                        }
                    }
                    t = fun;
                }
                T::TypeApply { fun, .. } => t = fun,
                _ => return false,
            }
        }
    }

    fn local(&mut self, name: String, flags: Flags, info: TypeId) -> SymbolId {
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
    /// `inlined`, the expansion of `call` with bindings `locals` and expansion type `expanded`,
    /// cast to the call's type if an opaque proxy hides that it conforms; for a transparent
    /// method to the call's type intersected with the expansion's type, the proxies mapped back
    /// to their object (dotty's `unpackProxiesFromResultType` in `InlineCall.expand`).
    pub fn sealed_opaques(
        &mut self,
        inlined: TNodeId,
        call: TNodeId,
        locals: &[SymbolId],
        expanded: TypeId,
    ) -> Result<TNodeId, Unsupported> {
        let proxied = locals.iter().copied().find_map(|s| {
            let mut module_ref = self.u.info(s);
            while let Type::Refined(parent, ..) = *self.u.types().get(module_ref) {
                module_ref = parent;
            }
            let Type::Sym(_, m) = *self.u.types().get(module_ref) else { return None };
            let class = self.u.module_class(m)?;
            let refined = module_ref != self.u.info(s);
            (self.u.symbol(s).flags.has(SYNTHETIC) && refined && self.u.has_opaques(class))
                .then_some((s, module_ref))
        });
        let Some((opaque, module_ref)) = proxied else { return Ok(inlined) };
        let opaque_ref = self.u.intern_type(Type::Sym(None, opaque));
        let proxies: Vec<SymbolId> =
            locals.iter().copied().filter(|&s| s == opaque || self.u.info(s) == opaque_ref).collect();
        let call_tpe = self.tpd.tpe(call);
        let meth = self.tpd.sym(self.tpd.meth_part(call));
        let target = if meth.is_some_and(|m| self.u.symbol(m).flags.has(TRANSPARENT)) {
            let unpacked = self.u.map_type(expanded, &mut |u, t| match *u.types().get(t) {
                Type::Sym(None, s) if proxies.contains(&s) => Some(module_ref),
                _ => None,
            });
            self.u.glb(call_tpe, unpacked)
        } else {
            call_tpe
        };
        if self.u.is_subtype(self.tpd.tpe(inlined), target) {
            return Ok(inlined);
        }
        let unstable = match *self.u.types().get(target) {
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term() && !self.pure().is_stable_member(s),
            _ => false,
        };
        let to = if unstable { self.u.widen(target) } else { target };
        let cast = self.cast(inlined, to)?;
        if let T::TypeApply { args, .. } = *self.tpd.tree(cast)
            && let Some(&tpt) = self.tpd.items(args).first()
        {
            let end = self.tpd.span(inlined).end_pos();
            self.tpd.get_mut(tpt).span = end;
        }
        Ok(cast)
    }

    /// Whether type `t` refers to the `this` of class `cls`.
    fn mentions_this(&mut self, t: TypeId, cls: SymbolId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::This(c) = *u.types().get(part) {
                found |= u.class_symbol(c) == Some(cls);
            }
            None
        });
        found
    }

    /// `module_ref` refined by the aliases of the opaque types of its class `module`, the last
    /// declared innermost (dotty's `OpaqueProxy.apply`).
    fn opaques_exposed(&mut self, module_ref: TypeId, module: SymbolId) -> TypeId {
        let decls = self.u.symbol(module).decls.clone();
        let mut refined = module_ref;
        for d in decls.into_iter().rev() {
            let s = self.u.symbol(d);
            if s.kind != Kind::Type || !s.flags.has(OPAQUE) {
                continue;
            }
            let Some(alias) = self.u.opaque_alias(d) else { continue };
            let name = s.name.clone();
            let seen = self.u.as_seen_from(alias, module_ref, module);
            let info = self.u.intern_type(Type::Alias(seen));
            refined = self.u.intern_type(Type::Refined(refined, name, info));
        }
        refined
    }
}
