//! Expected types of implicit searches with type variables outside the subset: unbounded ones,
//! and ones in unions.

use super::super::Typer;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether `t` has an uninstantiated type variable without bounds.
    pub fn has_unbounded_var(&mut self, t: TypeId) -> bool {
        let vars = self.open_vars(t);
        vars.into_iter().any(|v| self.constraint.bounds(v) == (tasty::NOTHING, tasty::ANY))
    }

    /// Whether `t`, its aliases expanded, has a union with an uninstantiated type variable in it.
    pub fn has_open_union(&mut self, t: TypeId) -> bool {
        self.open_union_within(t, 16)
    }

    fn open_union_within(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Or(..) => !self.open_vars(t).is_empty(),
            Type::And(a, b) => self.open_union_within(a, depth) || self.open_union_within(b, depth),
            Type::Applied(_, args) => args.into_iter().any(|a| self.open_union_within(a, depth)),
            Type::Annotated(a, _) | Type::ByName(a) => self.open_union_within(a, depth),
            Type::Bounds(lo, hi) => self.open_union_within(lo, depth) || self.open_union_within(hi, depth),
            _ => false,
        }
    }
}
