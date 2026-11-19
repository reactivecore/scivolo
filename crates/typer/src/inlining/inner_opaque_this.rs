// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The proxy of the `this` of a static object with opaque types for an inline call inside the
//! object (dotty's `canElideThis`, `registerType` and `computeThisBindings`).

use super::Unsupported;
use super::expand::Expansion;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{SymbolId, Type};

impl Expansion<'_, '_> {
    /// Whether the prefix of call reference `fun` of method `meth` is the `this` of `module`:
    /// written as such, or left out in a method of `module`.
    pub fn prefix_is_this_of(&mut self, fun: TNodeId, meth: SymbolId, module: SymbolId) -> bool {
        let Some(q) = self.call_prefix(fun) else { return self.i.u.symbol(meth).owner == Some(module) };
        let qt = self.i.tpd.tpe(q);
        self.i.u.this_of(qt).and_then(|c| self.i.u.class_symbol(c)) == Some(module)
    }

    /// Whether the expansion keeps references to static members through static prefixes: among
    /// the proxies `this`, one of a static object's `this` of that `this` type, and no `$proxyN`
    /// (dotty's `StopAt.Static` when `opaqueProxies` is empty).
    pub fn stops_at_static(&mut self, this: &[(SymbolId, SymbolId)]) -> bool {
        let static_this = this.iter().any(|&(cls, p)| {
            let info = self.i.u.info(p);
            self.i.u.is_static_owner(cls) && self.i.u.this_of(info).is_some()
        });
        static_this && self.opaque_refs.is_empty()
    }

    /// The proxy `C$_this` of the `this` of static object class `module`, of that `this` type
    /// and bound to the object `source`, for a call inside `module` through another prefix:
    /// `MajorVersion.reset` inside `object Test { opaque type V = Int; object MajorVersion }`.
    pub fn bind_inner_opaque_this(
        &mut self,
        module: SymbolId,
        source: SymbolId,
    ) -> Result<(SymbolId, SymbolId), Unsupported> {
        let point = Span::at(self.i.tpd.span(self.call).point());
        let class = self.i.u.intern_type(Type::Sym(None, module));
        let this = self.i.u.intern_type(Type::This(class));
        let proxy = self.this_proxy_symbol(module, this);
        let module_ref = crate::defn::type_ref(self.i.u, source);
        let rhs = self.i.singleton(module_ref, point).ok_or(self.err("object reference"))?;
        self.bind_proxy(proxy, rhs, point, self.opaque_refs.len());
        Ok((module, proxy))
    }
}
