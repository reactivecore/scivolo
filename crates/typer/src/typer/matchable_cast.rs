// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The `Any` to `Matchable` conversion of `adaptToSubType`: `tree.asInstanceOf[T & Matchable]`.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// `tree` cast to its widened type and `Matchable` if `pt` is bounded by `Matchable`, the
    /// widened type does not derive from it, and the intersection conforms to `pt`.
    pub fn matchable_cast(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let matchable = self.u.builtins().matchable;
        if !self.is_matchable_bound(pt, 8) {
            return None;
        }
        let widened = self.u.widen(self.tpd.tpe(tree));
        if self.u.class_symbol(widened).is_some_and(|c| self.u.derives_from(c, matchable)) {
            return None;
        }
        let tpe = self.tpd.tpe(tree);
        let expr = match *self.u.types().get(tpe) {
            Type::ByName(r) => r,
            _ => tpe,
        };
        let matchable_ref = crate::defn::type_ref(self.u, matchable);
        let target = self.u.intern_type(Type::And(expr, matchable_ref));
        if !self.conforms(target, pt) {
            return None;
        }
        let span = self.tpd.span(tree).to_synthetic();
        let cast = self.cast_to(tree, target, span).ok()?;
        if let T::TypeApply { args, .. } = *self.tpd.tree(cast)
            && let Some(&tpt) = self.tpd.items(args).first()
        {
            self.tpd.get_mut(tpt).span = span.end_pos();
        }
        Some(cast)
    }

    /// Whether `t` is `Matchable` or bounded by it (dotty's `isMatchableBound`).
    fn is_matchable_bound(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        match *self.u.types().get(t) {
            Type::And(l, r) => return self.is_matchable_bound(l, depth) || self.is_matchable_bound(r, depth),
            Type::Or(l, r, _) => {
                return self.is_matchable_bound(l, depth) && self.is_matchable_bound(r, depth);
            }
            _ => {}
        }
        let class = self.u.class_symbol(t);
        let hi = match *self.u.types().get(t) {
            Type::Var(v) => Some(self.constraint.bounds(v).1),
            Type::Sym(_, s) if class.is_none() => Some(self.u.info(s)),
            _ => None,
        };
        let hi = hi.map(|h| match *self.u.types().get(h) {
            Type::Bounds(_, hi) => hi,
            _ => h,
        });
        match hi {
            Some(hi) if hi != t => self.is_matchable_bound(hi, depth),
            _ => class == Some(self.u.builtins().matchable),
        }
    }
}
