// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! `scala.compiletime.testing.typeChecks` and `typeCheckErrors` (dotty's `Inlines.Intrinsics`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant as C;
use tasty::SymbolId;

impl Inlining<'_> {
    /// `typeChecks(code)`: whether the constant string `code` parses, types in the context of the
    /// outermost call and expands without errors.
    pub fn type_checks(&mut self, arg: TNodeId, owner: SymbolId, span: Span) -> Result<bool, Unsupported> {
        let mut code = arg;
        loop {
            code = match *self.tpd.tree(code) {
                T::Typed { expr, .. } => expr,
                T::Block { stats, expr } if stats.is_empty() => expr,
                T::Inlined { bindings, expansion, .. } if bindings.is_empty() => expansion,
                _ => break,
            };
        }
        let literal = match self.tpd.tree(code) {
            T::Literal { value } => Some(value.clone()),
            _ => None,
        };
        let tpe = self.tpd.tpe(arg);
        let Some(C::String(code)) = literal.or_else(|| self.constant_of_type(tpe)) else {
            return Err((span, "typeChecks of code that is not a constant string"));
        };
        let code = String::from_utf16_lossy(&code);
        let ctx = self.ctx.ok_or((span, "typeChecks without the call's context"))?;
        let mark = self.host.error_count();
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        let checks = match self.host.typed_code(lent, &code, ctx) {
            Some(typed) => {
                let (bodies, with_inits) = (self.bodies.clone(), self.with_inits.clone());
                let parts = ((&mut *self.u, &mut *self.tpd), (&mut *self.names, &mut *self.fresh));
                let mut inlining = Inlining::new(parts.0, parts.1, &mut *self.host, false);
                (inlining.bodies, inlining.with_inits) = (bodies, with_inits);
                inlining.transform(typed, owner)?;
                !self.host.errors_since(mark)
            }
            None => false,
        };
        Ok(checks)
    }

    /// The empty list of errors `typeCheckErrors(code)` call `call` returns for code without
    /// errors: `Nil` ascribed the call's type.
    pub fn no_errors(&mut self, call: TNodeId, span: Span) -> Result<TNodeId, Unsupported> {
        let immutable =
            self.u.package("scala.collection.immutable").ok_or((span, "scala.collection.immutable"))?;
        let nil =
            self.u.decl(immutable, "Nil").unwrap_or_default().into_iter().find(|&d| self.u.is_module(d));
        let nil = nil.ok_or((span, "scala.collection.immutable.Nil"))?;
        let mut r = super::Reflect { u: &mut *self.u, tpd: &mut *self.tpd, names: &mut *self.names, span };
        let nil = r.local_ident(nil).map_err(|what| (span, what))?;
        let tpe = self.tpd.tpe(call);
        let tpt = self.tpd.node(T::TypeTree { inferred: true }, span.to_synthetic(), Some(tpe), None);
        Ok(self.tpd.node(T::Typed { expr: nil, tpt }, span, Some(tpe), None))
    }
}
