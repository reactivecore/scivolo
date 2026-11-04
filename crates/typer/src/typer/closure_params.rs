// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The type of a function value's parameter: declared, or the expected function type's (dotty's
//! `typedFunctionValue`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::NodeId;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The type tree of parameter `p` declared with `tpt`, or `callee`, its type from the callee,
    /// or from `formal`, its type in the expected function type; `Ok(None)` if `p` has none,
    /// `Err` the refusal.
    pub fn closure_param_tpt(
        &mut self,
        (p, tpt): (NodeId, NodeId),
        formal: Option<TypeId>,
        callee: Option<TypeId>,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, TNodeId> {
        if !self.is_untyped_param(p) {
            return Ok(Some(self.typed_type(tpt, ctx)));
        }
        if let Some(t) = callee {
            return Ok(Some(self.type_tree(t, self.ast.span(p).end_pos(), formal.is_some())));
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
        let formal = match self.fully_defined(formal) {
            Ok(t) if !self.has_proto_wildcard(t) => t,
            Ok(_) => return Err(self.unsupported(p, "parameter type left open by the expected type")),
            Err(what) => return Err(self.unsupported(p, what)),
        };
        Ok(Some(self.type_tree(formal, self.ast.span(p).end_pos(), true)))
    }

    /// The function or single abstract method type in union or intersection
    /// `t`: the one side that has one, both sides of an intersection; `None` for any other type
    /// (dotty's `findFunctionType`).
    pub fn function_type_in(&mut self, t: TypeId) -> Option<TypeId> {
        let z = self.zonk(t);
        let d = self.u.dealias(z);
        let (a, b, and) = match *self.u.types().get(d) {
            Type::Or(a, b, _) => (a, b, false),
            Type::And(a, b) => (a, b, true),
            Type::Flexible(u) => return self.function_type_in(u),
            _ => return self.is_function_like(d).then_some(d),
        };
        match (self.function_type_in(a), self.function_type_in(b)) {
            (Some(f), None) | (None, Some(f)) => Some(f),
            (Some(f), Some(g)) if and && f == g => Some(f),
            (Some(f), Some(g)) if and => Some(self.u.intern_type(Type::And(f, g))),
            _ => None,
        }
    }

    /// An uninstantiated type variable deep in the unions and intersections of `t` with a function
    /// or single abstract method type in a bound (dotty's `instantiatableTypeVar`).
    pub fn instantiatable_var(&mut self, t: TypeId) -> Option<TypeId> {
        let z = self.zonk(t);
        let d = self.u.dealias(z);
        match *self.u.types().get(d) {
            Type::Or(a, b, _) | Type::And(a, b) => {
                self.instantiatable_var(a).or_else(|| self.instantiatable_var(b))
            }
            Type::Flexible(u) => self.instantiatable_var(u),
            Type::Var(v) => {
                let (lo, hi) = self.constraint.bounds(v);
                (self.has_function_type(lo) || self.has_function_type(hi)).then_some(d)
            }
            _ => None,
        }
    }

    fn has_function_type(&mut self, t: TypeId) -> bool {
        let z = self.zonk(t);
        let d = self.u.dealias(z);
        match *self.u.types().get(d) {
            Type::Or(a, b, _) | Type::And(a, b) => self.has_function_type(a) || self.has_function_type(b),
            Type::Flexible(u) => self.has_function_type(u),
            _ => self.is_function_like(d),
        }
    }

    /// Whether `t` is a function or single abstract method type.
    fn is_function_like(&mut self, t: TypeId) -> bool {
        self.is_function_type(t)
            || self.context_function_parts(t).is_some()
            || self.u.class_symbol(t).is_some_and(|c| self.sam_method(c).is_some())
    }
}
