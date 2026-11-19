// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections of members of an opaque type named through the path of its object, inside the
//! object.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::TypeId;

impl Typer<'_> {
    /// Casts `qual` to its widened type with references to enclosing objects with opaque types
    /// made their `this`, if that type has a member `name`: `x.length` for `x: o.T` inside
    /// `object o { opaque type T = String }` (dotty's `tryLiftToThis`).
    pub fn lifted_to_this(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> Option<TNodeId> {
        let widened = self.u.widen(self.tpd.tpe(qual));
        let lifted = self.u.lift_to_this(widened);
        if lifted == widened || self.type_members_in(lifted, name, ctx).is_empty() {
            return None;
        }
        Some(self.type_cast(qual, lifted))
    }

    /// Ascribes `pt` to `tree` if its type conforms to `pt` only through references lifted to
    /// `this` since the count of lifts was `lifts`: `g(n)` for `n: Int` and `def g(c: o.T)`
    /// inside `object o { opaque type T = Int }` (dotty's `OKwithOpaquesUsed` in `adapt`).
    pub fn opaque_ascribed(&mut self, tree: TNodeId, pt: TypeId, lifts: u64) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(tree);
        if self.u.opaque_lifts() == lifts || self.types().conforms_unlifted(tpe, pt) {
            return None;
        }
        let span = self.tpd.span(tree);
        let tpt = self.type_tree(pt, span.end_pos(), false);
        let typed = self.tpd.node(T::Typed { expr: tree, tpt }, span.to_synthetic(), Some(pt), None);
        self.opaque_ascriptions.insert(typed);
        Some(typed)
    }

    /// Drops the ascription `opaque_ascribed` inserted around a method's right-hand side `rhs`
    /// (dotty's `InsertedTyped` in `typedDefDef`).
    pub fn without_opaque_ascription(&self, rhs: TNodeId) -> TNodeId {
        match *self.tpd.tree(rhs) {
            T::Typed { expr, .. } if self.opaque_ascriptions.contains(&rhs) => expr,
            _ => rhs,
        }
    }
}
