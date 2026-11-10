// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! Type variables that bound each other made one (dotty's `ConstraintHandling.unify`).

use super::Constraint;
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, Universe, VarId};

impl Constraint {
    /// For `v <: w` where `w <: v` holds already: one of the two instantiated to the other, which
    /// takes the bounds of both; `w` is kept unless `v` is nested less deeply. False and unchanged
    /// if `w <: v` does not hold, the joined bounds are unsatisfiable or the removed one's bounds
    /// cannot do without the local symbols defined after the kept one.
    pub fn unify(&mut self, u: &mut Universe, v: VarId, w: VarId) -> bool {
        let (ev, ew) = (self.vars[v.0 as usize], self.vars[w.0 as usize]);
        if v == w || ew.instance.is_some() {
            return false;
        }
        let Some(hi_w) = without_var(u, ew.hi, v) else { return false };
        let hi_v = without_var(u, ev.hi, w).unwrap_or(ev.hi);
        let ((kept, ek, hk), (removed, er, hr)) = if ev.at.nesting < ew.at.nesting {
            ((v, ev, hi_v), (w, ew, hi_w))
        } else {
            ((w, ew, hi_w), (v, ev, hi_v))
        };
        let Some(lo_r) = self.avoided(u, er.lo, ek.at, 1) else { return false };
        let Some(hr) = self.avoided(u, hr, ek.at, -1) else { return false };
        let (lo, hi) = both_bounds(u, (ek.lo, hk), (lo_r, hr));
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
    match *u.ty(t) {
        Type::Var(x) if x == v => Some(ANY),
        Type::And(x, y) => match (without_var(u, x, v), without_var(u, y, v)) {
            (Some(x), _) => Some(meet(u, x, y)),
            (_, Some(y)) => Some(meet(u, x, y)),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `t` has a type variable among its parts.
pub fn has_var(u: &mut Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= u.var(part).is_some();
        None
    });
    found
}

/// The bounds within both `k` and `r`: one of them if it lies within the other, `r` first, else
/// the union of the lower and the intersection of the upper bounds (dotty's `TypeBounds.&`).
fn both_bounds(u: &mut Universe, k: (TypeId, TypeId), r: (TypeId, TypeId)) -> (TypeId, TypeId) {
    let within = |u: &mut Universe, a: (TypeId, TypeId), b: (TypeId, TypeId)| {
        (b.0 == NOTHING || u.is_subtype(b.0, a.0)) && (b.1 == ANY || u.is_subtype(a.1, b.1))
    };
    if within(u, r, k) {
        r
    } else if within(u, k, r) {
        k
    } else {
        (join(u, k.0, r.0), first_meet(u, k.1, r.1))
    }
}

/// The union of `a` and `b`, `a` if each is below the other (dotty's `lub`).
fn join(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    if b == NOTHING || u.is_subtype(b, a) {
        a
    } else if a == NOTHING || u.is_subtype(a, b) {
        b
    } else {
        u.lub(a, b)
    }
}

/// The intersection of `a` and `b`, `a` if each is below the other (dotty's `glb`).
fn first_meet(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    if b == ANY || u.is_subtype(a, b) { a } else { meet(u, a, b) }
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
