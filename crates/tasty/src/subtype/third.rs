//! The third try of a comparison (dotty's `thirdTry`), by the right-hand side: applied types,
//! variables, refinements, lambdas, unions, method types, bounds.

use super::Cmp;
use crate::types::{Type, TypeId, VarId};
use crate::{ANY, NOTHING};

impl Cmp<'_> {
    pub fn third_try(&mut self, a: TypeId, b: TypeId) -> bool {
        match self.u.types().get(b).clone() {
            Type::Applied(tycon, args) => self.compare_applied2(a, b, tycon, &args),
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(b) {
                Some(n2) => self.third_try_named(a, b, n2),
                None => self.fourth_try(a, b),
            },
            Type::Var(v) => self.var_above(a, b, v),
            Type::Refined(parent, name, info) => {
                self.recur(a, parent) && self.has_matching_member(a, &name, info) || self.fourth_try(a, b)
            }
            Type::Rec(_, parent) => self.recur(a, parent) || self.fourth_try(a, b),
            Type::Lambda { binder: b2, params: p2, result: r2 } => match self.u.types().get(a).clone() {
                Type::Lambda { binder: b1, params: p1, result: r1 } => {
                    self.compare_binders((b1, &p1, r1), (b2, &p2, r2)) || self.fourth_try(a, b)
                }
                _ => match self.eta_expand(a) {
                    Some(eta) => self.recur(eta, b) || self.fourth_try(a, b),
                    None => self.fourth_try(a, b),
                },
            },
            Type::Or(x, y) => self.below_union(a, b, x, y),
            Type::Method { params: p2, result: r2, implicit: i2, .. } => {
                match self.u.types().get(a).clone() {
                    Type::Method { params: p1, result: r1, implicit: i1, .. } => {
                        p1.len() == p2.len()
                            && (i1 || !i2)
                            && p1
                                .iter()
                                .zip(&p2)
                                .all(|(&(_, x), &(_, y))| self.recur_frozen(x, y) && self.recur_frozen(y, x))
                            && self.recur(r1, r2)
                    }
                    _ => false,
                }
            }
            Type::Poly { binder: b2, params: p2, result: r2 } => match self.u.types().get(a).clone() {
                Type::Poly { binder: b1, params: p1, result: r1 } => {
                    self.compare_binders((b1, &p1, r1), (b2, &p2, r2))
                }
                _ => false,
            },
            Type::ByName(r2) => match self.u.types().get(a).clone() {
                Type::ByName(r1) => self.recur(r1, r2),
                Type::Method { params, result, .. } if params.is_empty() => self.recur(result, r2),
                _ => self.fourth_try(a, b),
            },
            Type::Bounds(lo2, hi2) => match self.u.types().get(a).clone() {
                Type::Bounds(lo1, hi1) => {
                    (lo2 == NOTHING || self.recur(lo2, lo1)) && (hi2 == ANY || self.recur(hi1, hi2))
                }
                _ => false,
            },
            _ => self.fourth_try(a, b),
        }
    }

    /// `a <: v` for variable `b = v` without instance: through its lower bound, else by
    /// constraining it.
    fn var_above(&mut self, a: TypeId, b: TypeId, v: VarId) -> bool {
        let always = match self.frozen() {
            true => {
                let (lo, _) = self.vars.bounds(v);
                self.recur(a, lo)
            }
            false => self.recur_frozen(a, b),
        };
        if always {
            return true;
        }
        match self.frozen() {
            true => self.fourth_try(a, b),
            false => {
                let a = match self.u.types().get(a).clone() {
                    Type::ByName(r) => r,
                    _ => a,
                };
                self.vars.add_lower(self.u, v, a)
            }
        }
    }

    /// `a <: x | y`: through `a` widened or dealiased, `&` in the union distributed, else one
    /// of the alternatives.
    fn below_union(&mut self, a: TypeId, b: TypeId, x: TypeId, y: TypeId) -> bool {
        let widened = self.u.widen(a);
        if widened != a && self.recur(widened, b) {
            return true;
        }
        let dealiased = self.u.dealias(a);
        if dealiased != a {
            return self.recur(dealiased, b);
        }
        for (and, other, left) in [(x, y, true), (y, x, false)] {
            if let Type::And(p, q) = self.u.types().get(and).clone() {
                let union = |s: &mut Self, t| match left {
                    true => s.u.intern_type(Type::Or(t, other)),
                    false => s.u.intern_type(Type::Or(other, t)),
                };
                let (bp, bq) = (union(self, p), union(self, q));
                return self.recur(a, bp) && self.recur(a, bq);
            }
        }
        self.either(|s| s.recur(a, x), |s| s.recur(a, y)) || self.fourth_try(a, b)
    }
}
