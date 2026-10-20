// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! Type variables that bound each other made one (dotty's `ConstraintHandling.unify`).

use super::Constraint;
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, Universe, VarId};

impl Constraint {
    /// For `v <: w` where `w <: v` holds already: one of the two instantiated to the other, which
    /// takes the bounds of both; `w` is kept unless `v` was made before more symbols were defined.
    /// False and unchanged if `w <: v` does not hold, the joined bounds are unsatisfiable or the
    /// removed one's bounds cannot do without the local symbols defined after the kept one.
    pub fn unify(&mut self, u: &mut Universe, v: VarId, w: VarId) -> bool {
        let (ev, ew) = (self.vars[v.0 as usize], self.vars[w.0 as usize]);
        if v == w || ew.instance.is_some() {
            return false;
        }
        let Some(hi_w) = without_var(u, ew.hi, v) else { return false };
        let hi_v = without_var(u, ev.hi, w).unwrap_or(ev.hi);
        let ((kept, ek, hk), (removed, er, hr)) = if ev.born.0 < ew.born.0 {
            ((v, ev, hi_v), (w, ew, hi_w))
        } else {
            ((w, ew, hi_w), (v, ev, hi_v))
        };
        let Some(lo_r) = self.avoided(u, er.lo, ek.born, 1) else { return false };
        let Some(hr) = self.avoided(u, hr, ek.born, -1) else { return false };
        let lo = join(u, ek.lo, lo_r);
        let hi = meet(u, hk, hr);
        let mark = self.mark();
        let kept_ref = u.intern_type(Type::Var(kept));
        self.instantiate(removed, kept_ref);
        self.set(kept, lo, hi);
        self.vars[kept.0 as usize].hard |= er.hard;
        let ok = hi == ANY || self.satisfiable(u, lo, hi);
        if !ok {
            self.rollback(mark);
        }
        ok
    }
}

/// `t` without the variable `v` among the parts of its top-level intersection, if `v` is one.
fn without_var(u: &mut Universe, t: TypeId, v: VarId) -> Option<TypeId> {
    match *u.types().get(t) {
        Type::Var(x) if x == v => Some(ANY),
        Type::And(x, y) => match (without_var(u, x, v), without_var(u, y, v)) {
            (Some(x), _) => Some(meet(u, x, y)),
            (_, Some(y)) => Some(meet(u, x, y)),
            _ => None,
        },
        _ => None,
    }
}

fn join(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    if a == NOTHING || u.is_subtype(a, b) {
        b
    } else if b == NOTHING || u.is_subtype(b, a) {
        a
    } else {
        u.lub(a, b)
    }
}

/// The intersection of `a` and `b`, one of them if it is below the other.
pub fn meet(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    if a == ANY || u.is_subtype(b, a) {
        b
    } else if b == ANY || u.is_subtype(a, b) {
        a
    } else {
        u.glb(a, b)
    }
}
