// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Implicit methods as values of a function type (dotty's `candidateKind` for a `FunctionN`
//! goal, and `typedImplicit` adapting the reference).

use super::super::{Pt, Typer};
use super::search::MISFITS;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::diag::Level;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Type `t` of implicit `sym` as a value compared with `pt`: its implicit clauses stripped, a
    /// method as a function type if `pt` is a `FunctionN` type (dotty's `normalize`); `None` if
    /// it stays a method.
    pub fn implicit_value_type(&mut self, t: TypeId, sym: SymbolId, pt: TypeId) -> Option<TypeId> {
        let t = self.strip_implicit(t);
        let method = matches!(self.u.types().get(t), Type::Method { .. });
        let t = match method && self.is_function_type(pt) {
            true => self.proto_normalized((t, Some(sym)), pt, true, 8)?,
            false => t,
        };
        (!matches!(self.u.types().get(t), Type::Method { .. } | Type::Poly { .. })).then_some(t)
    }

    /// Method reference `tree` eta-expanded to function type `pt`; a misfit if it refers to an
    /// inline implicit method, typing the function reports an error, or fails by a misfit.
    pub fn eta_expanded_implicit(
        &mut self,
        tree: TNodeId,
        pt: TypeId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        if self.is_inline_implicit_ref(tree) {
            return Err("implicit of another type");
        }
        let (outer, diags) = (self.unsupported.take(), self.diags.items.len());
        let adapted = self.adapt(tree, Pt::Type(pt), ctx);
        let inner = std::mem::replace(&mut self.unsupported, outer);
        if self.diags.items[diags..].iter().any(|d| d.level == Level::Error) {
            self.diags.items.truncate(diags);
            return Err("implicit of another type");
        }
        let Some((span, what)) = inner else { return Ok(adapted) };
        if let Some(misfit) = MISFITS.iter().find(|&&m| m == what) {
            return Err(misfit);
        }
        self.unsupported.get_or_insert((span, what));
        Err("implicit method as a function value")
    }
}
