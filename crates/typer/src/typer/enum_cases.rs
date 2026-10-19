// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Calls of enum case `apply` and `copy` typed as the enum (dotty's `widenEnumCase`).

use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use tasty::SymbolId;
use tasty::tags::{CASE, ENUM, SYNTHETIC};

impl Typer<'_> {
    fn is_enum_case(&self, c: SymbolId) -> bool {
        let flags = self.u.symbol(c).flags;
        flags.has(ENUM) && flags.has(CASE) && self.u.is_class(c)
    }

    /// Call `apply` of a synthetic `apply` or `copy` of an enum case class ascribed the case's
    /// parents, transparent traits dropped, if that conforms to `pt` (dotty's `widenEnumCase`).
    pub fn widen_enum_case(
        &mut self,
        apply: TNodeId,
        method: Option<SymbolId>,
        pt: Pt,
    ) -> Result<TNodeId, &'static str> {
        let Some(m) = method.filter(|&m| self.u.symbol(m).flags.has(SYNTHETIC)) else { return Ok(apply) };
        let Some(owner) = self.u.symbol(m).owner else { return Ok(apply) };
        let applies = match self.u.symbol(m).name.as_str() {
            "apply" => self.u.companion_class(owner).is_some_and(|c| self.is_enum_case(c)),
            "copy" => self.is_enum_case(owner),
            _ => false,
        };
        let tpe = self.tpd.tpe(apply);
        let Some(class) = self.u.class_symbol(tpe).filter(|&c| applies && self.is_enum_case(c)) else {
            return Ok(apply);
        };
        if !self.u.type_params(class).is_empty() {
            return Err("generic enum case");
        }
        let parents = self.u.parents(class);
        let widened = parents.into_iter().fold(tasty::ANY, |acc, p| self.u.glb(acc, p));
        let bound = match pt {
            Pt::Type(p) => p,
            _ => tasty::ANY,
        };
        let widened = self.without_transparent_traits(widened, bound);
        if !self.conforms(widened, bound) {
            return Ok(apply);
        }
        let span = self.tpd.span(apply);
        let tpt = self.type_tree(widened, span.end_pos(), true);
        Ok(self.tpd.node(T::Typed { expr: apply, tpt }, span.to_synthetic(), Some(widened), None))
    }

    /// The enum case call that `qual` ascribes its parents, a qualifier whose selection the
    /// widened type does not offer (dotty's `widenEnumCase` against a selection prototype).
    pub fn widened_enum_case(&mut self, qual: TNodeId) -> Option<TNodeId> {
        let T::Typed { expr, tpt } = *self.tpd.tree(qual) else { return None };
        if !matches!(self.tpd.tree(tpt), T::TypeTree { inferred: true })
            || !matches!(self.tpd.tree(expr), T::Apply { .. })
        {
            return None;
        }
        let tpe = self.tpd.tpe(expr);
        let class = self.u.class_symbol(tpe)?;
        self.is_enum_case(class).then_some(expr)
    }
}
