// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Applied types (dotty's `compareAppliedType2`, `compareAppliedType1` and `isSubArgs`).

use super::Cmp;
use super::refs::Meaning;
use crate::types::{Type, TypeId};
use crate::{ANY, Kind, NOTHING};

impl Cmp<'_> {
    /// `a <: tycon[args]`.
    pub fn compare_applied2(&mut self, a: TypeId, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        let variances = self.variances(tycon, args.len());
        if self.matching_apply(a, tycon, args, &variances) || self.compare_compiletime(b, a, true) {
            return true;
        }
        match *self.u.types().get(tycon) {
            Type::Var(v) => match self.vars.instance(v) {
                Some(t) => {
                    let applied = self.u.intern_type(Type::Applied(t, args.to_vec()));
                    self.recur(a, applied)
                }
                None => {
                    if self.var_constructor_above(a, tycon, args) {
                        return true;
                    }
                    let (lo, hi) = self.vars.bounds(v);
                    let lower = self.apply(lo, args);
                    match lo == hi {
                        true => self.recur(a, lower),
                        false => self.either(|s| s.fourth_try(a, b), |s| s.recur(a, lower)),
                    }
                }
            },
            Type::TypeRef(..) | Type::Sym(..) => match self.named(tycon).map(|n| n.meaning) {
                Some(Meaning::Alias(alias)) => match self.expanded(tycon, alias, args) {
                    Some(expanded) if expanded != b => self.recur(a, expanded),
                    _ => self.fourth_try(a, b),
                },
                Some(Meaning::Abstract(lo, hi)) => {
                    let lower = self.apply(lo, args);
                    let declared = match lo == hi {
                        true => self.recur(a, lower),
                        false => self.either(|s| s.fourth_try(a, b), |s| s.recur(a, lower)),
                    };
                    declared
                        || self.gadt_applied(tycon, args, false).is_some_and(|glo| self.recur_frozen(a, glo))
                }
                Some(Meaning::Class(c)) => match self.nested_pairs(c, args) {
                    Some(pairs) => self.try_base_type(a, b, c) || self.recur(a, pairs),
                    None => self.try_base_type(a, b, c),
                },
                _ => self.fourth_try(a, b),
            },
            Type::Annotated(inner, _) => {
                let applied = self.u.intern_type(Type::Applied(inner, args.to_vec()));
                self.recur(a, applied)
            }
            Type::Lambda { .. } => {
                let reduced = self.u.dealias(b);
                reduced != b && self.recur(a, reduced)
            }
            Type::Applied(..) => self.compare_curried2(a, b, tycon, args),
            _ => self.fourth_try(a, b),
        }
    }

    /// Whether `a` widened applies the same constructor as `tycon2` to arguments that conform to
    /// `args2` (dotty's `isMatchingApply`).
    fn matching_apply(&mut self, a: TypeId, tycon2: TypeId, args2: &[TypeId], variances: &[i8]) -> bool {
        let widened = self.u.widen(a);
        let Type::Applied(mut tycon1, args1) = self.u.types().get(widened).clone() else { return false };
        loop {
            match *self.u.types().get(tycon1) {
                Type::Var(v) => match self.vars.instance(v) {
                    Some(t) => tycon1 = t,
                    None => {
                        let same = tycon1 == tycon2 || !self.frozen() && self.recur(tycon1, tycon2);
                        return same && self.sub_args(&args1, args2, variances, &[]);
                    }
                },
                Type::Annotated(inner, _) => tycon1 = inner,
                Type::TypeRef(..) | Type::Sym(..) => {
                    let (Some(n1), Some(n2)) = (self.named(tycon1), self.named(tycon2)) else { return false };
                    let same = match (n1.sym, n2.sym) {
                        (Some(s1), Some(s2)) => s1 == s2 && self.sub_prefix_of(s1, n1.prefix, n2.prefix),
                        (None, None) => n1.same_unresolved(&n2),
                        _ => false,
                    };
                    let tparams = match n2.sym {
                        Some(s) if self.u.symbol(s).kind == Kind::Class => self.u.type_params(s),
                        _ => Vec::new(),
                    };
                    return same && self.sub_args(&args1, args2, variances, &tparams);
                }
                Type::Param(..) => return tycon1 == tycon2 && self.sub_args(&args1, args2, variances, &[]),
                _ => return false,
            }
        }
    }

    /// `tycon[args] <: b`.
    pub fn compare_applied1(&mut self, a: TypeId, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        if self.compare_compiletime(a, b, false) {
            return true;
        }
        match *self.u.types().get(tycon) {
            Type::Var(v) => {
                let upper = match self.vars.instance(v) {
                    Some(t) => self.u.intern_type(Type::Applied(t, args.to_vec())),
                    None if self.var_constructor_below(tycon, args, b) => return true,
                    None => {
                        let (_, hi) = self.vars.bounds(v);
                        self.apply(hi, args)
                    }
                };
                self.recur(upper, b)
            }
            Type::TypeRef(..) | Type::Sym(..) => match self.named(tycon).map(|n| n.meaning) {
                Some(Meaning::Alias(alias)) => match self.expanded(tycon, alias, args) {
                    Some(expanded) if expanded != a => self.recur(expanded, b),
                    _ => false,
                },
                Some(Meaning::Abstract(_, hi)) => {
                    let upper = self.apply(hi, args);
                    upper != a && self.recur(upper, b)
                        || self.gadt_applied(tycon, args, true).is_some_and(|ghi| self.recur_frozen(ghi, b))
                }
                _ => false,
            },
            Type::Annotated(inner, _) => {
                let applied = self.u.intern_type(Type::Applied(inner, args.to_vec()));
                self.recur(applied, b)
            }
            Type::Lambda { .. } => {
                let reduced = self.u.dealias(a);
                reduced != a && self.recur(reduced, b)
            }
            Type::Applied(..) => self.compare_curried1(a, b, tycon, args),
            _ => false,
        }
    }

    /// Abstract type constructor `tycon`'s GADT upper bound if `upper`, else its lower bound
    /// unless `Nothing`, applied to `args`.
    fn gadt_applied(&mut self, tycon: TypeId, args: &[TypeId], upper: bool) -> Option<TypeId> {
        let (lo, hi) = self.gadt_bounds(tycon)?;
        let bound = if upper { hi } else { lo };
        (bound != NOTHING && bound != tycon).then(|| self.apply(bound, args))
    }

    /// The variances of the `n` type parameters of `tycon`, +1, -1 or 0; invariant where unknown.
    pub fn variances(&mut self, tycon: TypeId, n: usize) -> Vec<i8> {
        match self.named(tycon).and_then(|n| n.sym) {
            Some(s) => match self.u.variances(s) {
                v if v.len() == n => v,
                _ => vec![0; n],
            },
            None => vec![0; n],
        }
    }

    /// `tycon[args]` for alias `tycon` of `alias`, a refinement's type lambda if not a symbol's.
    fn expanded(&mut self, tycon: TypeId, alias: TypeId, args: &[TypeId]) -> Option<TypeId> {
        let lambda = matches!(self.u.types().get(alias), Type::Lambda { .. });
        self.u.expand(tycon, args).or_else(|| lambda.then(|| self.apply(alias, args)))
    }

    /// `t` applied to `args`: a type lambda's body, `t` itself if it takes no arguments.
    pub fn apply(&mut self, t: TypeId, args: &[TypeId]) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::Lambda { binder, params, result } => {
                let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                self.u.subst(result, binder, &names, args)
            }
            _ if args.is_empty() || t == NOTHING || t == ANY => t,
            _ => self.u.intern_type(Type::Applied(t, args.to_vec())),
        }
    }
}
