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
        match *self.u.ty(tycon) {
            Type::Var(v) => match self.vars.instance(v) {
                Some(t) => {
                    let applied = self.u.intern_applied(t, args);
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
                _ if self.gadt_applied_below(a, b, tycon, args) => true,
                Some(Meaning::Alias(alias)) => match self.expanded(tycon, alias, args) {
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
                Some(Meaning::Class(c)) => match self.nested_pairs(c, args) {
                    Some(pairs) => self.recur(a, pairs) || self.try_base_type(a, b, c),
                    None => self.try_base_type(a, b, c),
                },
                _ => self.fourth_try(a, b),
            },
            Type::Annotated(inner, _) => {
                let applied = self.u.intern_applied(inner, args);
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
        let Some((mut tycon1, args1)) = self.u.applied(widened) else { return false };
        let args1 = args1.to_vec();
        loop {
            match *self.u.ty(tycon1) {
                Type::Var(v) => match self.vars.instance(v) {
                    Some(t) => tycon1 = t,
                    None => {
                        let same = tycon1 == tycon2 || self.can_constrain(v) && self.recur(tycon1, tycon2);
                        return same && self.sub_args(&args1, args2, variances, &[]);
                    }
                },
                Type::Annotated(inner, _) => tycon1 = inner,
                Type::TypeRef(..) | Type::Sym(..) => {
                    let (Some(n1), Some(n2)) = (self.named(tycon1), self.named(tycon2)) else { return false };
                    let (same, by_gadt) = match (n1.sym, n2.sym) {
                        (Some(s1), Some(s2)) if s1 == s2 && self.sub_prefix_of(s1, n1.prefix, n2.prefix) => {
                            (true, None)
                        }
                        (Some(_), Some(_)) => match self.same_by_gadt(tycon1, tycon2) {
                            Some(instantiated) => (true, Some(instantiated)),
                            None => (false, None),
                        },
                        (None, None) => (n1.same_unresolved(&n2), None),
                        _ => (false, None),
                    };
                    if !same {
                        return false;
                    }
                    let tparams = match n2.sym {
                        Some(s) if self.u.symbol(s).kind == Kind::Class => self.u.type_params(s),
                        _ => Vec::new(),
                    };
                    let is_class = |c: &Self, n: Option<crate::SymbolId>| n.is_some_and(|s| c.u.is_class(s));
                    let injective =
                        (is_class(self, n1.sym) || is_class(self, n2.sym)) && by_gadt != Some(false);
                    return match injective {
                        true => self.sub_args(&args1, args2, variances, &tparams),
                        false => self.in_frozen_gadt(|c| c.sub_args(&args1, args2, variances, &tparams)),
                    };
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
        match *self.u.ty(tycon) {
            Type::Var(v) => {
                let upper = match self.vars.instance(v) {
                    Some(t) => self.u.intern_applied(t, args),
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
                    Some(expanded) if expanded != a => {
                        self.recur(expanded, b) || self.gadt_applied_above(b, tycon, args)
                    }
                    _ => self.gadt_applied_above(b, tycon, args),
                },
                Some(Meaning::Abstract(_, hi)) => {
                    let upper = self.apply(hi, args);
                    upper != a && self.recur(upper, b) || self.gadt_applied_above(b, tycon, args)
                }
                _ => false,
            },
            Type::Annotated(inner, _) => {
                let applied = self.u.intern_applied(inner, args);
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

    /// `tycon[args]` for alias `tycon` of `alias`: a type lambda's body, `alias[args]` for an
    /// alias of a type constructor (dotty's `superTypeNormalized`, `applyIfParameterized`).
    fn expanded(&mut self, tycon: TypeId, alias: TypeId, args: &[TypeId]) -> Option<TypeId> {
        self.u.expand(tycon, args).or_else(|| (!args.is_empty()).then(|| self.apply(alias, args)))
    }

    /// `t` applied to `args`: a type lambda's body, `t` itself if it takes no arguments.
    pub fn apply(&mut self, t: TypeId, args: &[TypeId]) -> TypeId {
        match self.u.lambda(t) {
            Some(l) => {
                let (binder, names, result) = (l.binder, l.names(), l.result);
                self.u.subst(result, binder, &names, args)
            }
            None if args.is_empty() || t == NOTHING || t == ANY => t,
            None => self.u.intern_applied(t, args),
        }
    }
}
