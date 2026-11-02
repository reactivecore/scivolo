// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! The declared bounds of variables and the direction they are instantiated in by them (dotty's
//! `instDirection`).

use super::Constraint;
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, Universe, VarId};

impl Constraint {
    /// Records `lo` and `hi` as the declared bounds of `t` if it is a variable.
    pub fn declare(&mut self, u: &Universe, t: TypeId, lo: TypeId, hi: TypeId) {
        if let Type::Var(v) = *u.types().get(t) {
            self.declared.resize(self.vars.len(), (NOTHING, ANY));
            self.declared[v.0 as usize] = (lo, hi);
        }
    }

    /// 1 (maximize) if `v`'s bounds are narrower than its declared ones only from above, -1
    /// (minimize) if only from below, else 0.
    pub fn direction(&mut self, u: &mut Universe, v: VarId) -> i8 {
        let (lo, hi) = self.bounds(v);
        let (declared_lo, declared_hi) = self.declared.get(v.0 as usize).copied().unwrap_or((NOTHING, ANY));
        let below = !u.is_subtype_frozen_in(lo, declared_lo, self);
        let above = !u.is_subtype_frozen_in(declared_hi, hi, self);
        above as i8 - below as i8
    }
}
