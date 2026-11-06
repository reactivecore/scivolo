// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala
//! The type parameter occurring in a type where its variance does not allow it (dotty's
//! `VarianceChecker.Validator` and `checkLambda`, over `TypeAccumulator.foldOver`).

use super::Typer;
use crate::sealed::is_contained_in;
use tasty::tags::{CONTRAVARIANT as CONTRA_TAG, COVARIANT as CO_TAG};
use tasty::{Binder, SymbolId, Type, TypeId, Universe};

pub const INVARIANT: u8 = 0;
pub const COVARIANT: u8 = 1;
pub const CONTRAVARIANT: u8 = 2;
pub const BIVARIANT: u8 = 3;

const MAX_DEPTH: u32 = 64;

pub fn flip(v: u8) -> u8 {
    (v & COVARIANT) << 1 | (v & CONTRAVARIANT) >> 1
}

pub fn label(v: u8) -> &'static str {
    match v {
        COVARIANT => "covariant",
        CONTRAVARIANT => "contravariant",
        BIVARIANT => "bivariant",
        _ => "invariant",
    }
}

/// The variance type parameter `s` is declared with.
pub fn declared(u: &Universe, s: SymbolId) -> u8 {
    let flags = u.symbol(s).flags;
    u8::from(flags.has(CO_TAG)) | (u8::from(flags.has(CONTRA_TAG)) << 1)
}

/// Whether annotation class type `annot` is `@uncheckedVariance`.
pub fn is_unchecked_variance(u: &mut Universe, annot: TypeId) -> bool {
    let class = u.class_symbol(annot);
    class.is_some_and(|c| u.full_name(c) == "scala.annotation.unchecked.uncheckedVariance")
}

pub fn of_sign(v: i8) -> u8 {
    match v {
        1 => COVARIANT,
        -1 => CONTRAVARIANT,
        _ => INVARIANT,
    }
}

/// What is checked: the type parameters of enclosing classes in the type of a definition, or the
/// parameters of a type lambda with their declared variances.
#[derive(Clone, Copy)]
pub enum Checked<'a> {
    Definition(SymbolId),
    Lambda(Binder, &'a [i8]),
}

/// A type parameter where its variance does not allow it and the variance required there: an
/// enclosing class's, or a lambda's by index.
pub enum Misplaced {
    Class(SymbolId, u8),
    Lambda(usize, u8),
}

impl Typer<'_> {
    /// The first type parameter that `checked` restricts occurring in `t` at variance `v` where
    /// its own does not allow it (dotty's `Validator.apply`, `checkLambda`'s `checkOK`).
    pub fn misplaced(&mut self, checked: Checked, t: TypeId, v: i8, depth: u32) -> Option<Misplaced> {
        if depth > MAX_DEPTH {
            return None;
        }
        let go = |s: &mut Self, t, v| s.misplaced(checked, t, v, depth + 1);
        match (self.u.types().get(t).clone(), checked) {
            (Type::Param(b, i), Checked::Lambda(binder, variances)) if b == binder => {
                let own = variances.get(i as usize).copied().unwrap_or(0);
                (own != 0 && own != v).then_some(Misplaced::Lambda(i as usize, of_sign(v)))
            }
            (Type::Sym(p, _), Checked::Lambda(..)) => p.and_then(|p| go(self, p, v.max(0))),
            (Type::Sym(p, _), Checked::Definition(base)) => self.misplaced_ref(base, t, p, v, depth),
            (Type::TypeRef(p, _), Checked::Definition(base)) => {
                self.misplaced_ref(base, t, Some(p), v, depth)
            }
            (Type::TypeRef(p, _) | Type::TermRef(p, _), _) => go(self, p, v.max(0)),
            (Type::Applied(tycon, args), _) => {
                let variances = match (self.u.types().get(tycon).clone(), checked) {
                    (Type::Param(b, i), Checked::Definition(base)) => self.poly_param_variances(base, b, i),
                    _ => self.tycon_variances(tycon),
                };
                go(self, tycon, v).or_else(|| {
                    args.iter().enumerate().find_map(|(i, &a)| {
                        let av = match self.u.types().get(a) {
                            Type::Bounds(..) | Type::Wildcard(_) => v,
                            _ => v * variances.get(i).copied().unwrap_or(0),
                        };
                        go(self, a, av)
                    })
                })
            }
            (Type::Annotated(_, annot), _) if is_unchecked_variance(self.u, annot) => None,
            (Type::Annotated(inner, _), _) => go(self, inner, v),
            (Type::Bounds(lo, hi), _) if lo == hi => go(self, lo, 0),
            (Type::Bounds(lo, hi), _) => go(self, lo, -v).or_else(|| go(self, hi, v)),
            (Type::Alias(a), _) => go(self, a, 0),
            (Type::And(a, b) | Type::Or(a, b, _) | Type::Super(a, b), _) => {
                go(self, a, v).or_else(|| go(self, b, v))
            }
            (Type::Refined(p, _, info), _) => go(self, p, v).or_else(|| go(self, info, v)),
            (Type::Method { params, result, .. } | Type::Poly { params, result, .. }, _) => {
                params.iter().find_map(|&(_, p)| go(self, p, -v)).or_else(|| go(self, result, v))
            }
            (Type::Lambda { params, result, .. }, _) => {
                let pv = if matches!(self.u.types().get(result), Type::MatchCase(..)) { 0 } else { -v };
                params.iter().find_map(|&(_, p)| go(self, p, pv)).or_else(|| go(self, result, v))
            }
            (Type::Match { bound, scrutinee, cases }, _) => go(self, bound, v)
                .or_else(|| go(self, scrutinee, 0))
                .or_else(|| cases.iter().find_map(|&c| go(self, c, v))),
            (Type::MatchCase(pat, body), _) => go(self, pat, 0).or_else(|| go(self, body, v)),
            (
                Type::Flexible(x)
                | Type::ByName(x)
                | Type::Rec(_, x)
                | Type::Skolem(_, x)
                | Type::Wildcard(Some(x)),
                _,
            ) => go(self, x, v),
            _ => None,
        }
    }

    /// The variances the bounds of type parameter `i` of polymorphic type `binder` in the type
    /// of `base` declare.
    fn poly_param_variances(&mut self, base: SymbolId, binder: Binder, i: u32) -> Vec<i8> {
        let mut info = self.u.info(base);
        loop {
            match self.u.types().get(info).clone() {
                Type::Poly { binder: b, params, .. } if b == binder => {
                    let Some(&(_, bounds)) = params.get(i as usize) else { return Vec::new() };
                    let hi = match *self.u.types().get(bounds) {
                        Type::Bounds(_, hi) => hi,
                        _ => bounds,
                    };
                    let Type::Lambda { binder: lambda, .. } = *self.u.types().get(hi) else {
                        return Vec::new();
                    };
                    return self.u.declared_variances(lambda).unwrap_or_default();
                }
                Type::Poly { result, .. } | Type::Method { result, .. } => info = result,
                _ => return Vec::new(),
            }
        }
    }

    /// `misplaced` for reference `t` with prefix `prefix` in the type of definition `base`.
    fn misplaced_ref(
        &mut self,
        base: SymbolId,
        t: TypeId,
        prefix: Option<TypeId>,
        v: i8,
        depth: u32,
    ) -> Option<Misplaced> {
        let checked = Checked::Definition(base);
        if let Some(s) = self.u.referenced(t) {
            let owner = self.u.symbol(s).owner;
            if declared(self.u, s) != INVARIANT && owner.is_some_and(|o| is_contained_in(self.u, base, o)) {
                return self.misplaced_symbol(s, base, v);
            }
            if let Some(alias) = self.u.alias_of(s)
                && !matches!(self.u.types().get(alias), Type::Match { .. })
            {
                return self.misplaced(checked, alias, v, depth + 1);
            }
        }
        prefix.and_then(|p| self.misplaced(checked, p, v.max(0), depth + 1))
    }

    /// `tvar` with the variance required of it where it occurs at variance `v` in the type of
    /// `base`, if its own is not that (dotty's `checkVarianceOfSymbol`).
    fn misplaced_symbol(&mut self, tvar: SymbolId, base: SymbolId, v: i8) -> Option<Misplaced> {
        let relative = self.relative_variance(tvar, base, COVARIANT);
        if relative == BIVARIANT {
            return None;
        }
        let required = match v {
            1 => relative,
            -1 => flip(relative),
            _ => INVARIANT,
        };
        (declared(self.u, tvar) & required == 0).then_some(Misplaced::Class(tvar, required))
    }
}
