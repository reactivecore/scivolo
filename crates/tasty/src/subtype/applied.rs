//! Applied types (dotty's `compareAppliedType2`, `compareAppliedType1` and `isSubArgs`).

use super::Cmp;
use super::refs::Meaning;
use crate::types::{Type, TypeId};

impl Cmp<'_> {
    /// `a <: tycon[args]`.
    pub fn compare_applied2(&mut self, a: TypeId, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        let variances = self.variances(tycon, args.len());
        if self.matching_apply(a, tycon, args, &variances) {
            return true;
        }
        match self.u.types().get(tycon).clone() {
            Type::Var(v) => match self.vars.instance(v) {
                Some(t) => {
                    let applied = self.u.intern_type(Type::Applied(t, args.to_vec()));
                    self.recur(a, applied)
                }
                None => {
                    let (lo, hi) = self.vars.bounds(v);
                    let lower = self.apply(lo, args);
                    match lo == hi {
                        true => self.recur(a, lower),
                        false => self.either(|s| s.fourth_try(a, b), |s| s.recur(a, lower)),
                    }
                }
            },
            Type::TypeRef(..) | Type::Sym(..) => match self.named(tycon).map(|n| n.meaning) {
                Some(Meaning::Alias(_)) => match self.u.expand(tycon, args) {
                    Some(expanded) if expanded != b => self.recur(a, expanded),
                    _ => self.fourth_try(a, b),
                },
                Some(Meaning::Abstract(lo, hi)) => {
                    let lower = self.apply(lo, args);
                    match lo == hi {
                        true => self.recur(a, lower),
                        false => self.either(|s| s.fourth_try(a, b), |s| s.recur(a, lower)),
                    }
                }
                Some(Meaning::Class(c)) => self.try_base_type(a, b, c),
                _ => self.fourth_try(a, b),
            },
            Type::Annotated(inner) => {
                let applied = self.u.intern_type(Type::Applied(inner, args.to_vec()));
                self.recur(a, applied)
            }
            Type::Lambda { .. } => {
                let reduced = self.u.dealias(b);
                reduced != b && self.recur(a, reduced)
            }
            _ => self.fourth_try(a, b),
        }
    }

    /// Whether `a` widened applies the same constructor as `tycon2` to arguments that conform to
    /// `args2` (dotty's `isMatchingApply`).
    fn matching_apply(&mut self, a: TypeId, tycon2: TypeId, args2: &[TypeId], variances: &[i8]) -> bool {
        let widened = self.u.widen(a);
        let Type::Applied(mut tycon1, args1) = self.u.types().get(widened).clone() else { return false };
        loop {
            match self.u.types().get(tycon1).clone() {
                Type::Var(v) => match self.vars.instance(v) {
                    Some(t) => tycon1 = t,
                    None => {
                        let same = tycon1 == tycon2 || !self.frozen() && self.recur(tycon1, tycon2);
                        return same && self.sub_args(&args1, args2, variances);
                    }
                },
                Type::Annotated(inner) => tycon1 = inner,
                Type::TypeRef(..) | Type::Sym(..) => {
                    let (Some(n1), Some(n2)) = (self.named(tycon1), self.named(tycon2)) else { return false };
                    let same = match (n1.sym, n2.sym) {
                        (Some(s1), Some(s2)) => s1 == s2 && self.sub_prefix(n1.prefix, n2.prefix),
                        (None, None) => n1.meaning == n2.meaning,
                        _ => false,
                    };
                    return same && self.sub_args(&args1, args2, variances);
                }
                _ => return false,
            }
        }
    }

    /// `tycon[args] <: b`.
    pub fn compare_applied1(&mut self, a: TypeId, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        match self.u.types().get(tycon).clone() {
            Type::Var(v) => {
                let upper = match self.vars.instance(v) {
                    Some(t) => self.u.intern_type(Type::Applied(t, args.to_vec())),
                    None => {
                        let (_, hi) = self.vars.bounds(v);
                        self.apply(hi, args)
                    }
                };
                self.recur(upper, b)
            }
            Type::TypeRef(..) | Type::Sym(..) => match self.named(tycon).map(|n| n.meaning) {
                Some(Meaning::Alias(_)) => match self.u.expand(tycon, args) {
                    Some(expanded) if expanded != a => self.recur(expanded, b),
                    _ => false,
                },
                Some(Meaning::Abstract(_, hi)) => {
                    let upper = self.apply(hi, args);
                    upper != a && self.recur(upper, b)
                }
                _ => false,
            },
            Type::Annotated(inner) => {
                let applied = self.u.intern_type(Type::Applied(inner, args.to_vec()));
                self.recur(applied, b)
            }
            Type::Lambda { .. } => {
                let reduced = self.u.dealias(a);
                reduced != a && self.recur(reduced, b)
            }
            _ => false,
        }
    }

    /// Whether each of `args1` conforms to the argument of `args2` at its place, by the
    /// variance of the parameter there.
    fn sub_args(&mut self, args1: &[TypeId], args2: &[TypeId], variances: &[i8]) -> bool {
        args1.len() == args2.len()
            && args1
                .iter()
                .zip(args2)
                .enumerate()
                .all(|(i, (&x, &y))| self.sub_arg(x, y, variances.get(i).copied().unwrap_or(0)))
    }

    fn sub_arg(&mut self, x: TypeId, y: TypeId, variance: i8) -> bool {
        match (self.arg_bounds(x), self.arg_bounds(y)) {
            (_, Some((lo2, hi2))) => match self.arg_bounds(x) {
                Some((lo1, hi1)) => self.recur(lo2, lo1) && self.recur(hi1, hi2),
                None => self.recur(lo2, x) && self.recur(x, hi2),
            },
            (Some((lo1, hi1)), None) => match variance {
                1 => self.recur(hi1, y),
                -1 => self.recur(y, lo1),
                _ => false,
            },
            (None, None) => match variance {
                1 => self.recur(x, y),
                -1 => self.recur(y, x),
                _ => self.recur(x, y) && self.recur(y, x),
            },
        }
    }

    /// The bounds of a wildcard argument.
    fn arg_bounds(&mut self, t: TypeId) -> Option<(TypeId, TypeId)> {
        match self.u.types().get(t).clone() {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            Type::Wildcard(None) => Some((crate::NOTHING, crate::ANY)),
            Type::Wildcard(Some(b)) => self.arg_bounds(b),
            _ => None,
        }
    }

    /// The variances of the `n` type parameters of `tycon`, +1, -1 or 0; invariant where unknown.
    fn variances(&mut self, tycon: TypeId, n: usize) -> Vec<i8> {
        match self.named(tycon).and_then(|n| n.sym) {
            Some(s) => match self.u.variances(s) {
                v if v.len() == n => v,
                _ => vec![0; n],
            },
            None => vec![0; n],
        }
    }

    /// `t` applied to `args`: a type lambda's body, `t` itself if it takes no arguments.
    fn apply(&mut self, t: TypeId, args: &[TypeId]) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::Lambda { binder, params, result } => {
                let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                self.u.subst(result, binder, &names, args)
            }
            _ if args.is_empty() || t == crate::NOTHING || t == crate::ANY => t,
            _ => self.u.intern_type(Type::Applied(t, args.to_vec())),
        }
    }
}
