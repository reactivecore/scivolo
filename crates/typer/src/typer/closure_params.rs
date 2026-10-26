// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The type of a function value's parameter: declared, or the expected function type's (dotty's
//! `typedFunctionValue`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::NodeId;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The type tree of parameter `p` declared with `tpt`, or from `formal`, its type in the
    /// expected function type, in a closure that is `contextual`; `Ok(None)` if `p` has neither,
    /// `Err` the refusal.
    pub fn closure_param_tpt(
        &mut self,
        (p, tpt): (NodeId, NodeId),
        formal: Option<TypeId>,
        contextual: bool,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, TNodeId> {
        if !self.is_untyped_param(p) {
            return Ok(Some(self.typed_type(tpt, ctx)));
        }
        let Some(formal) = formal else { return Ok(None) };
        let formal = self.zonk(formal);
        let formal = self.without_erased_param(formal);
        let lower = match *self.u.types().get(formal) {
            Type::Bounds(lo, _) => Some(lo),
            _ => None,
        };
        let nothing = Some(self.u.builtins().nothing);
        if lower.is_some_and(|lo| self.u.class_symbol(lo) == nothing) {
            return Err(self.unsupported(p, "parameter type inferred from the target of a wildcard"));
        }
        let formal = lower.unwrap_or(formal);
        if contextual && !self.is_fully_defined(formal) {
            return Err(self.unsupported(p, "context closure parameter of an open type"));
        }
        let formal = match self.fully_defined(formal) {
            Ok(t) if !self.has_proto_wildcard(t) => t,
            Ok(_) => return Err(self.unsupported(p, "parameter type left open by the expected type")),
            Err(what) => return Err(self.unsupported(p, what)),
        };
        Ok(Some(self.type_tree(formal, self.ast.span(p).end_pos(), true)))
    }
}
