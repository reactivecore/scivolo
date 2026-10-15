//! Type variables that bound each other made one (dotty's `ConstraintHandling.unify`), and bounds
//! without the local symbols defined after a variable (`LevelAvoidMap`).

use super::Constraint;
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

impl Constraint {
    /// For `v <: w` where `w <: v` holds already: one of the two instantiated to the other, which
    /// takes the bounds of both; `w` is kept unless `v` was made before more symbols were defined.
    /// False and unchanged if `w <: v` does not hold, the joined bounds are unsatisfiable or the
    /// removed one's bounds need a local symbol defined after the kept one.
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
        let Some(lo_r) = avoided(u, er.lo, ek.born) else { return false };
        if avoided(u, hr, ek.born) != Some(hr) {
            return false;
        }
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

/// `t` with the local classes defined from `born` on replaced by their parents and the local
/// values by their types; `None` if another local symbol defined from then remains.
pub fn avoided(u: &mut Universe, t: TypeId, born: SymbolId) -> Option<TypeId> {
    let mut t = t;
    for _ in 0..4 {
        let mut found = false;
        t = u.map_type(t, &mut |u, part| {
            let Type::Sym(_, s) = *u.types().get(part) else { return None };
            if !is_local_since(u, s, born) {
                return None;
            }
            found = true;
            match u.symbol(s).kind {
                Kind::Class => Some(u.parents(s).into_iter().fold(ANY, |acc, p| meet(u, acc, p))),
                Kind::Val | Kind::Param => Some(u.widen(part)),
                _ => None,
            }
        });
        if !found {
            return Some(t);
        }
    }
    None
}

/// Whether `s` is defined from `born` on in a method or value.
fn is_local_since(u: &Universe, s: SymbolId, born: SymbolId) -> bool {
    s.0 >= born.0 && u.symbol(s).owner.is_some_and(|o| matches!(u.symbol(o).kind, Kind::Def | Kind::Val))
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

fn meet(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    if a == ANY || u.is_subtype(b, a) {
        b
    } else if b == ANY || u.is_subtype(a, b) {
        a
    } else {
        u.glb(a, b)
    }
}
