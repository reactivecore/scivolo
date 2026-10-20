// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Least upper bounds of types by their atoms (dotty's `mergedLub`, its step on atoms).

use super::atoms::Atoms;
use super::{Cmp, NoVars};
use crate::Universe;
use crate::types::{Type, TypeId};

impl Universe {
    /// The least upper bound of `a` and `b` by their atoms if both have known atoms and one
    /// contains the other or they are disjoint; `None` otherwise.
    pub fn atoms_lub(&mut self, a: TypeId, b: TypeId) -> Option<TypeId> {
        Cmp::new(self, &mut NoVars).atoms_lub(a, b, true)
    }
}

impl Cmp<'_> {
    /// `b` if `a` may only have atoms `b` must have, `a` likewise, their union (soft if `soft`)
    /// if they have no atom in common; `None` if either's atoms are unknown or they overlap
    /// otherwise.
    pub fn atoms_lub(&mut self, a: TypeId, b: TypeId, soft: bool) -> Option<TypeId> {
        let outer = std::mem::replace(&mut self.provisional, false);
        let (atoms_a, atoms_b) = (self.atoms(a), self.atoms(b));
        let fixed = !self.provisional;
        self.provisional |= outer;
        let (Atoms::Known(r1), Atoms::Known(r2)) = (&atoms_a, &atoms_b) else { return None };
        let us = self.u.unions();
        if us.subset(&r1.hi, &r2.lo) {
            return Some(b);
        }
        if us.subset(&r2.hi, &r1.lo) {
            return Some(a);
        }
        if !us.disjoint(&r1.hi, &r2.hi) {
            return None;
        }
        let t = self.or_type(a, b, soft);
        let parts_kept = self.is_nothing(a) == Some(false) && self.is_nothing(b) == Some(false);
        if fixed && parts_kept && matches!(self.u.types().get(t), Type::Or(..)) {
            let atoms = self.joined(atoms_a, atoms_b, true);
            self.u.unions().set_atoms(t, atoms);
        }
        Some(t)
    }
}
