// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! Bounds without wildcards (dotty's `LevelAvoidMap.mapWild`).

use super::{At, Constraint};
use tasty::{ANY, NOTHING, Type, TypeId, Universe};

impl Constraint {
    /// Bound `t` with each wildcard replaced by a fresh variable made `at`, bounded by the
    /// wildcard's bounds without wildcards in turn.
    pub fn without_wildcards(&mut self, u: &mut Universe, t: TypeId, at: At) -> TypeId {
        if !has_wildcard(u, t) {
            return t;
        }
        u.map_type(t, &mut |u, part| {
            let Type::Wildcard(bounds) = *u.ty(part) else { return None };
            let (lo, hi) = bounds.and_then(|b| u.bounds(b)).unwrap_or((NOTHING, ANY));
            let (lo, hi) = (self.without_wildcards(u, lo, at), self.without_wildcards(u, hi, at));
            let var = self.fresh_at(u, at);
            let v = u.var(var).expect("a fresh variable");
            self.set(v, lo, hi);
            Some(var)
        })
    }
}

fn has_wildcard(u: &mut Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= matches!(u.ty(part), Type::Wildcard(_));
        None
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty::{TypeVars, VarId};

    fn var(c: &mut Constraint, u: &mut Universe) -> VarId {
        let t = c.fresh(u);
        u.var(t).expect("a fresh variable")
    }

    #[test]
    fn a_wildcard_bound_becomes_a_fresh_variable_within_its_bounds() {
        let (mut c, mut u) = (Constraint::default(), Universe::empty());
        let v = var(&mut c, &mut u);
        let inner = u.intern_type(Type::Wildcard(None));
        let bounds = u.intern_type(Type::Bounds(NOTHING, inner));
        let wildcard = u.intern_type(Type::Wildcard(Some(bounds)));
        assert!(c.add_lower(&mut u, v, wildcard));
        let lo = c.bounds(v).0;
        let w = u.var(lo).expect("a variable as the lower bound");
        assert_ne!(w, v);
        assert!(c.is_pending(w));
        let (w_lo, w_hi) = c.bounds(w);
        assert_eq!(w_lo, NOTHING);
        let x = u.var(w_hi).expect("the inner wildcard as a variable");
        assert_eq!(c.bounds(x), (NOTHING, ANY));
    }
}
