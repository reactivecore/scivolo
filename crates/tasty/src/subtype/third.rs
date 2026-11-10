// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The third try of a comparison (dotty's `thirdTry`), by the right-hand side: applied types,
//! variables, refinements, lambdas, unions, method types, bounds.

use super::Cmp;
use crate::types::{Binder, Type, TypeId, VarId};
use crate::{ANY, NOTHING};

impl Cmp<'_> {
    pub fn third_try(&mut self, a: TypeId, b: TypeId) -> bool {
        match *self.u.ty(b) {
            Type::Applied(tycon, args) => {
                let args = self.u.types_of(args).to_vec();
                self.compare_applied2(a, b, tycon, &args)
            }
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(b) {
                Some(n2) => self.third_try_named(a, b, n2),
                None => self.fourth_try(a, b),
            },
            Type::Var(v) => self.var_above(a, b, v),
            Type::Flexible(hi) => {
                let lo = self.u.or_null(hi);
                self.recur(a, lo)
            }
            Type::Refined(parent, name, info) => {
                self.recur(a, parent) && self.has_matching_member(a, name, info) || self.fourth_try(a, b)
            }
            Type::Rec(binder, parent) => self.compare_rec(a, binder, parent) || self.fourth_try(a, b),
            Type::Match { .. } => match self.reduced(b) {
                Some(r) => self.recur(a, r),
                None => self.fourth_try(a, b),
            },
            Type::Lambda { binder: b2, params: p2, result: r2 } => match *self.u.ty(a) {
                Type::Lambda { binder: b1, params: p1, result: r1 } => {
                    let (p1, p2) = (self.u.params_of(p1).to_vec(), self.u.params_of(p2).to_vec());
                    self.compare_binders((b1, &p1, r1), (b2, &p2, r2)) || self.fourth_try(a, b)
                }
                _ => match self.eta_expand(a) {
                    Some(eta) => self.recur(eta, b) || self.fourth_try(a, b),
                    None => self.fourth_try(a, b),
                },
            },
            Type::Or(x, y, _) => match self.compare_atoms(a, b, false) {
                Some(decided) => decided,
                None => self.below_union(a, b, x, y),
            },
            Type::Method { binder: b2, params: p2, result: r2, implicit: i2, .. } => match *self.u.ty(a) {
                Type::Method { binder: b1, params: p1, result: r1, implicit: i1, .. } => {
                    let (p1, p2) = (self.u.params_of(p1).to_vec(), self.u.params_of(p2).to_vec());
                    if p1.len() != p2.len() || !i1 && i2 {
                        return false;
                    }
                    let names: Vec<_> = p2.iter().map(|&(n, _)| n).collect();
                    let refs: Vec<TypeId> =
                        (0..p1.len() as u32).map(|i| self.u.intern_type(Type::Param(b1, i))).collect();
                    let own = |s: &mut Self, t: TypeId| s.u.param_syms_as_refs(t, &p1, b1);
                    let (x, r1): (Vec<TypeId>, TypeId) =
                        (p1.iter().map(|&(_, x)| own(self, x)).collect(), own(self, r1));
                    let theirs = |s: &mut Self, t: TypeId| {
                        let t = s.u.param_syms_as_refs(t, &p2, b2);
                        s.u.subst(t, b2, &names, &refs)
                    };
                    let y: Vec<TypeId> = p2.iter().map(|&(_, y)| theirs(self, y)).collect();
                    let r2 = theirs(self, r2);
                    if !x.iter().zip(&y).all(|(&x, &y)| self.recur_frozen(x, y) && self.recur_frozen(y, x)) {
                        return false;
                    }
                    self.recur(r1, r2)
                }
                _ => false,
            },
            Type::Poly { binder: b2, params: p2, result: r2 } => match *self.u.ty(a) {
                Type::Poly { binder: b1, params: p1, result: r1 } => {
                    let (p1, p2) = (self.u.params_of(p1).to_vec(), self.u.params_of(p2).to_vec());
                    self.compare_binders((b1, &p1, r1), (b2, &p2, r2))
                }
                _ => false,
            },
            Type::ByName(r2) => match *self.u.ty(a) {
                Type::ByName(r1) => self.recur(r1, r2),
                Type::Method { params, result, .. } if self.u.params_of(params).is_empty() => {
                    self.recur(result, r2)
                }
                _ => self.fourth_try(a, b),
            },
            Type::Bounds(lo2, hi2) => match *self.u.ty(a) {
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
                let a = match *self.u.ty(a) {
                    Type::ByName(r) => r,
                    Type::Applied(tycon, args) => {
                        let args = self.u.types_of(args).to_vec();
                        self.applied_alias_expansion(tycon, &args).unwrap_or(a)
                    }
                    _ => a,
                };
                self.vars.add_lower(self.u, v, a)
            }
        }
    }

    /// `a <:` recursive type `binder`, `parent`: the parent of recursive `a` against `parent` with
    /// one `this`, else stable `a` or a skolem of `a` against `parent` with that as `this` (dotty's
    /// `compareRec`).
    fn compare_rec(&mut self, a: TypeId, binder: Binder, parent: TypeId) -> bool {
        let d = self.u.dealias(a);
        let rec = match *self.u.ty(d) {
            Type::Rec(b1, p1) => Some((b1, p1)),
            _ => None,
        };
        let (a, this) = match rec {
            Some((b1, p1)) => (p1, self.u.intern_type(Type::RecThis(b1))),
            _ if self.is_stable(a) => (a, a),
            _ => {
                let skolem = self.u.fresh_skolem(a);
                (skolem, skolem)
            }
        };
        let opened = self.u.map_type(parent, &mut |u, x| (*u.ty(x) == Type::RecThis(binder)).then_some(this));
        self.recur(a, opened)
    }

    /// `tycon[args]` expanded if `tycon` is a type alias (dotty's `superType` of an applied alias).
    fn applied_alias_expansion(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        match self.named(tycon)?.meaning {
            super::refs::Meaning::Alias(_) => self.u.expand(tycon, args),
            _ => None,
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
        let soft = matches!(self.u.ty(b), Type::Or(_, _, true));
        for (and, other, left) in [(x, y, true), (y, x, false)] {
            if let Type::And(p, q) = *self.u.ty(and) {
                let union = |s: &mut Self, t| match left {
                    true => s.u.intern_type(Type::Or(t, other, soft)),
                    false => s.u.intern_type(Type::Or(other, t, soft)),
                };
                let (bp, bq) = (union(self, p), union(self, q));
                return self.recur(a, bp) && self.recur(a, bq);
            }
        }
        self.either(|s| s.recur(a, x), |s| s.recur(a, y)) || self.fourth_try(a, b)
    }
}
