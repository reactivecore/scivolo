// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Closures given a single abstract method type (dotty's `typedClosure`, `toSAM`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::TypeId;

impl Typer<'_> {
    /// The type a closure of parameter types `params` and result `result` implementing single
    /// abstract method type `parent` gets: `parent` with its type variables instantiated, a
    /// `PartialFunction` of `params` and `result` if `parent` is one left open by the expected
    /// type (dotty's `typedClosure`).
    pub fn closure_parent(
        &mut self,
        parent: TypeId,
        params: &[TypeId],
        result: TypeId,
    ) -> Result<TypeId, &'static str> {
        if self.is_fully_defined(parent) && !self.has_proto_wildcard(parent) {
            return Ok(parent);
        }
        if !self.has_proto_wildcard(parent) {
            return self.fully_defined_as(parent, true);
        }
        let [param] = params[..] else { return Err("underspecified single abstract method type") };
        self.partial_function_of(param, result)
    }

    /// Function value `tree` typed as single abstract method type `pt` if it does not conform but
    /// fits the method (dotty's `toSAM`).
    pub fn to_sam(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let T::Block { stats, expr } = *self.tpd.tree(tree) else { return None };
        let T::Closure { env, meth, tpt: None } = *self.tpd.tree(expr) else { return None };
        let tpe = self.tpd.tpe(tree);
        if self.u.is_subtype(tpe, pt) {
            return None;
        }
        let n = (0..=22).find(|&n| self.function_parts(tpe, n).is_some())?;
        let parent = self.sam_parts(pt, n, false).ok()??.parent;
        let span = self.tpd.span(expr);
        let tpt = Some(self.type_tree(parent, span.start_pos(), false));
        let closure = self.tpd.node(T::Closure { env, meth, tpt }, span, Some(parent), None);
        let span = self.tpd.span(tree);
        Some(self.build().block(stats, closure, span))
    }
}
