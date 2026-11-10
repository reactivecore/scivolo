// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//! The type aliases export clauses define for type members (dotty's `addForwarder` of a type).

use super::exports::{Export, Forwarder};
use crate::typer::Typer;
use syntax::ast::Span;
use syntax::ast::flags as f;
use tasty::tags as t;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether `s` is a type alias an export clause defines.
    pub fn is_exported_type(&self, s: SymbolId) -> bool {
        self.u.symbol(s).kind == Kind::Type && self.u.symbol(s).flags.has(t::EXPORTED)
    }

    /// Whether `s` is a type alias an export clause defines for a class that gets constructor
    /// proxies (dotty's `needsConstructorProxies`).
    pub fn is_exported_class(&mut self, s: SymbolId) -> bool {
        if !self.is_exported_type(s) {
            return false;
        }
        let info = self.u.info(s);
        let Some(mut target) = self.u.type_alias(info) else { return false };
        if let Some(lambda) = self.u.lambda(target) {
            target = lambda.result;
        }
        if let Some((tycon, _)) = self.u.applied(target) {
            target = tycon;
        }
        let target = self.u.dealias(target);
        self.u.class_symbol(target).is_some_and(|c| self.needs_constructor_proxies(c))
    }

    /// The type alias forwarding to type member `m` of `e`'s path.
    pub fn add_type_forwarder(&mut self, e: &mut Export, alias: &str, m: SymbolId, span: Span) {
        let class = self.u.symbol(m).kind == Kind::Class;
        let taken =
            self.u.decl(e.cls, alias).unwrap_or_default().into_iter().any(|d| self.u.is_type_symbol(d));
        if taken {
            self.unsupported_at(span, "export of a type whose name the class defines");
            return;
        }
        let mut target = self.u.intern_type(Type::Sym(Some(e.path), m));
        if self.refers_to_private(e.path) {
            let dealiased = self.u.dealias(target);
            if dealiased == target {
                self.unsupported_at(span, "export of a type through a private path");
                return;
            }
            target = dealiased;
        }
        let Some(target) = self.eta_expanded_member(m, (e.path, target), class) else {
            self.unsupported_at(span, "export of a type constructor");
            return;
        };
        let fl = f::EXPORTED.or(f::FINAL).or(if self.u.symbol(m).flags.has(t::INFIX) {
            f::INFIX
        } else {
            f::EMPTY
        });
        let sym = self.syms.define(self.u, alias, Kind::Type, e.cls, fl);
        let info = self.u.intern_type(Type::Alias(target));
        self.u.set_info(sym, info);
        e.out.push(Forwarder { sym, target: m, span });
    }

    /// `target`, a reference to type member `m`, as a type lambda applying it to its type
    /// parameters if it has some, else itself (dotty's `etaExpand`).
    fn eta_expanded_member(
        &mut self,
        m: SymbolId,
        (path, target): (TypeId, TypeId),
        class: bool,
    ) -> Option<TypeId> {
        self.complete(m);
        let lambda = if class {
            if self.u.type_params(m).is_empty() {
                return Some(target);
            }
            self.eta_expanded(m, target).ok()?
        } else {
            let info = self.u.info(m);
            let lambda =
                (self.u.type_alias(info)).or_else(|| self.u.bounds(info).map(|(_, l)| l)).unwrap_or(info);
            let Some((old, params)) = self.u.lambda(lambda).map(|l| (l.binder, l.params.to_vec())) else {
                return Some(target);
            };
            let binder = self.u.fresh_binder();
            let rebind = |u: &mut tasty::Universe, t: TypeId| {
                u.map_type(t, &mut |u, x| match u.param_ref(x) {
                    Some((b, i)) if b == old => Some(u.intern_type(Type::Param(binder, i))),
                    _ => None,
                })
            };
            let params: Vec<(String, TypeId)> =
                params.into_iter().map(|(n, b)| (n, rebind(self.u, b))).collect();
            let args = (0..params.len() as u32).map(|i| self.u.intern_type(Type::Param(binder, i))).collect();
            let result = self.u.intern_type(Type::Applied(target, args));
            self.u.intern_type(Type::Lambda { binder, params, result })
        };
        let owner = self.u.symbol(m).owner?;
        let lambda = self.u.as_seen_from(lambda, path, owner);
        let variances = self.u.variances(m);
        if let Some(binder) = self.u.lambda(lambda).map(|l| l.binder)
            && variances.iter().any(|&v| v != 0)
        {
            self.u.declare_lambda_variances(binder, variances);
        }
        Some(lambda)
    }
}
