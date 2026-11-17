// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Widening abstract types against the captures of a legacy match case (dotty's
//! `widenAbstractOKFor`, `canWidenAbstract`), and where captures occur.

use super::super::Cmp;
use super::super::refs::Meaning;
use crate::types::{Type, TypeId};

/// How many aliases and bounds `has_class_ref` follows at most.
const MAX_CLASS_REF_STEPS: u32 = 64;
/// The variances of a match case's pattern and body.
const MATCH_CASE_VARIANCES: [i8; 2] = [0, 1];

impl Cmp<'_> {
    /// Whether an abstract type may widen to its bound to be compared with `pat` (dotty's
    /// `widenAbstractOKFor`): always outside a legacy case; within one only on its second
    /// attempt, the captures at variant positions of `pat` then noted as poisoned.
    pub fn widen_abstract_ok(&mut self, pat: TypeId) -> bool {
        match &self.legacy {
            None => return true,
            Some(l) if !l.can_widen_abstract => return false,
            Some(_) => {}
        }
        let mut found = Vec::new();
        self.capture_variances(pat, 1, &mut found);
        if let Some(l) = self.legacy.as_mut() {
            for (i, variance) in found {
                l.poison(i, variance);
            }
        }
        true
    }

    /// Whether a match type may widen to its bound outside a first legacy attempt.
    pub fn widen_match_ok(&self) -> bool {
        self.legacy.as_ref().is_none_or(|l| l.can_widen_abstract)
    }

    /// Whether `a` may be compared with `b` through a base type (dotty's condition in
    /// `tryBaseType`): outside a legacy case, if widening against `b` is allowed, or if `a`
    /// widens to a class reference.
    pub fn base_type_ok(&mut self, a: TypeId, b: TypeId) -> bool {
        if self.legacy.is_none() || self.widen_abstract_ok(b) {
            return true;
        }
        let widened = self.u.widen(a);
        self.has_class_ref(widened)
    }

    /// Whether `t` stands for a class reference, applied or refined (dotty's
    /// `underlyingClassRef(refinementOK = true).exists`).
    fn has_class_ref(&mut self, mut t: TypeId) -> bool {
        for _ in 0..MAX_CLASS_REF_STEPS {
            t = self.u.dealias(t);
            t = match *self.u.ty(t) {
                Type::Sym(..) | Type::TypeRef(..) => {
                    return matches!(self.named(t).map(|n| n.meaning), Some(Meaning::Class(_)));
                }
                Type::Applied(tycon, args) => {
                    if matches!(self.named(tycon).map(|n| n.meaning), Some(Meaning::Class(_))) {
                        return true;
                    }
                    let args = self.u.types_of(args).to_vec();
                    let Some(sup) = self.applied_super_type(tycon, &args) else { return false };
                    sup
                }
                Type::Annotated(p, _) | Type::Refined(p, ..) | Type::Rec(_, p) => p,
                _ => return false,
            };
        }
        false
    }

    /// Adds to `out` each occurrence in `t`, at `variance`, of a capture of the legacy case
    /// being matched, with its variance (dotty's `TypeAccumulator` variances).
    pub fn capture_variances(&mut self, t: TypeId, variance: i8, out: &mut Vec<(usize, i8)>) {
        let Some(l) = &self.legacy else { return };
        match *self.u.ty(t) {
            Type::Var(v) => out.extend(l.index(v).map(|i| (i, variance))),
            Type::Applied(tycon, args) => {
                let args = self.u.types_of(args).to_vec();
                let variances = self.u.tycon_variances(tycon, args.len());
                self.capture_variances(tycon, variance, out);
                for (a, v) in args.into_iter().zip(variances) {
                    self.capture_variances(a, variance * v, out);
                }
            }
            Type::Bounds(lo, hi) if lo == hi => self.capture_variances(lo, 0, out),
            Type::Bounds(lo, hi) => {
                self.capture_variances(lo, -variance, out);
                self.capture_variances(hi, variance, out);
            }
            Type::Alias(a) => self.capture_variances(a, 0, out),
            Type::And(a, b) | Type::Or(a, b, _) => {
                self.capture_variances(a, variance, out);
                self.capture_variances(b, variance, out);
            }
            Type::Refined(p, _, info) => {
                self.capture_variances(p, variance, out);
                self.capture_variances(info, variance, out);
            }
            Type::MatchCase(pat, body) => {
                self.capture_variances(pat, variance * MATCH_CASE_VARIANCES[0], out);
                self.capture_variances(body, variance * MATCH_CASE_VARIANCES[1], out);
            }
            Type::Annotated(p, _) | Type::Flexible(p) | Type::ByName(p) | Type::Rec(_, p) => {
                self.capture_variances(p, variance, out)
            }
            Type::Lambda { params, result, .. } => {
                for (_, b) in self.u.params_of(params).to_vec() {
                    self.capture_variances(b, -variance, out);
                }
                self.capture_variances(result, variance, out);
            }
            Type::Match { bound, scrutinee, cases } => {
                self.capture_variances(bound, variance, out);
                self.capture_variances(scrutinee, 0, out);
                for case in self.u.types_of(cases).to_vec() {
                    self.capture_variances(case, variance, out);
                }
            }
            _ => self.invariant_captures(t, out),
        }
    }

    /// Adds to `out` each capture `t` mentions, as invariant.
    fn invariant_captures(&mut self, t: TypeId, out: &mut Vec<(usize, i8)>) {
        let Some(l) = &self.legacy else { return };
        let mut found = Vec::new();
        self.u.map_type(t, &mut |u, x| {
            found.extend(u.var(x).and_then(|v| l.index(v)).map(|i| (i, 0)));
            None
        });
        out.extend(found);
    }
}
