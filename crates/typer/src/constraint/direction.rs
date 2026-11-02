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
        let below = lo != declared_lo && !self.frozen_subtype(u, lo, declared_lo);
        let above = hi != declared_hi && !self.frozen_subtype(u, declared_hi, hi);
        above as i8 - below as i8
    }

    /// Whether `a <: b` holds without narrowing the bounds of variables.
    fn frozen_subtype(&mut self, u: &mut Universe, a: TypeId, b: TypeId) -> bool {
        let (mark, escaped) = (self.mark(), self.escaped);
        let ok = u.is_subtype_in(a, b, self);
        self.rollback(mark);
        self.escaped = escaped;
        ok
    }
}
