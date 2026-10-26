// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Matching a scrutinee against the cases of a match type (dotty's `MatchReducer.matchCases`).

use super::super::Cmp;
use super::spec::{Pattern, Spec, Specced};
use crate::SymbolId;
use crate::tags::INTconst;
use crate::types::{Type, TypeId};

/// The outcome of matching one case (dotty's `MatchResult`); `Stuck` also stands for
/// `NoInstance` and `ReducedAndDisjoint`.
enum Outcome {
    Reduced(TypeId),
    Disjoint,
    Stuck,
}

/// The captures found so far, and whether one has no specific instance.
struct Instances {
    of: Vec<Option<TypeId>>,
    not_specific: bool,
}

impl Cmp<'_> {
    /// The body of the first case `scrut` matches, all cases before it disjoint from it.
    pub fn match_cases(&mut self, scrut: TypeId, cases: &[TypeId]) -> Option<TypeId> {
        for &case in cases {
            let spec = self.analyze(case);
            let outcome = match spec {
                Spec::SubTypeTest(pat, body) => self.match_sub_type_test(scrut, pat, body),
                Spec::Specced(s) => self.match_specced(scrut, &s),
                Spec::Unsupported => Outcome::Stuck,
            };
            match outcome {
                Outcome::Disjoint => continue,
                Outcome::Reduced(t) => return Some(t),
                Outcome::Stuck => return None,
            }
        }
        None
    }

    fn match_sub_type_test(&mut self, scrut: TypeId, pat: TypeId, body: TypeId) -> Outcome {
        let disjoint = self.provably_disjoint(scrut, pat);
        match (self.recur(scrut, pat), disjoint) {
            (true, false) => Outcome::Reduced(body),
            (false, true) => Outcome::Disjoint,
            _ => Outcome::Stuck,
        }
    }

    fn match_specced(&mut self, scrut: TypeId, s: &Specced) -> Outcome {
        let mut inst = Instances { of: vec![None; s.names.len()], not_specific: false };
        let bounded = self.u.subst(s.pat, s.binder, &s.names, &s.bounds);
        let disjoint = self.provably_disjoint(scrut, bounded);
        let fallback = if disjoint { Outcome::Disjoint } else { Outcome::Stuck };
        if !self.match_pattern(&s.pattern, scrut, 1, false, &mut inst) {
            return fallback;
        }
        let Some(args) = inst.of.iter().copied().collect::<Option<Vec<TypeId>>>() else {
            return Outcome::Stuck;
        };
        if inst.not_specific {
            return Outcome::Stuck;
        }
        let pat = self.u.subst(s.pat, s.binder, &s.names, &args);
        match self.recur(scrut, pat) {
            true if disjoint => Outcome::Stuck,
            true => Outcome::Reduced(self.u.subst(s.body, s.binder, &s.names, &args)),
            false => fallback,
        }
    }

    /// Whether `scrut` matches `pattern` structurally, its captures recorded in `inst`.
    fn match_pattern(
        &mut self,
        pattern: &Pattern,
        scrut: TypeId,
        variance: i8,
        widened: bool,
        inst: &mut Instances,
    ) -> bool {
        let bounds = self.arg_bounds(scrut);
        match pattern {
            Pattern::Capture(i, true) => {
                let t = match (variance, bounds) {
                    (1, Some((_, hi))) => hi,
                    (-1, Some((lo, _))) => lo,
                    _ => scrut,
                };
                inst.of[*i as usize] = Some(t);
                !matches!(self.u.types().get(t), Type::Error)
            }
            Pattern::Capture(i, false) => {
                let t = match bounds {
                    Some((lo, hi)) if !widened && variance != 0 => {
                        if variance == 1 {
                            hi
                        } else {
                            lo
                        }
                    }
                    Some(_) => {
                        inst.not_specific = true;
                        scrut
                    }
                    None if widened && variance != 0 => {
                        inst.not_specific = true;
                        scrut
                    }
                    None => scrut,
                };
                inst.of[*i as usize] = Some(t);
                !matches!(self.u.types().get(t), Type::Error)
            }
            Pattern::TypeTest => true,
            Pattern::BaseTypeTest { class, args, needs_concrete } => {
                let Some(base) = self.match_base_type(scrut, *class) else { return false };
                let Type::Applied(_, base_args) = self.u.types().get(base).clone() else { return false };
                let widened = widened || *needs_concrete && !self.is_concrete(scrut, 32);
                let variances = self.u.variances(*class);
                args.len() == base_args.len()
                    && args
                        .iter()
                        .zip(&base_args)
                        .zip(&variances)
                        .all(|((p, &a), &v)| self.match_pattern(p, a, v, widened, inst))
            }
            Pattern::AbstractTypeConstructor { tycon, sym, args } => {
                let dealiased = self.u.dealias(scrut);
                let Type::Applied(t, scrut_args) = self.u.types().get(dealiased).clone() else {
                    return false;
                };
                let (Some(n1), Some(n2)) = (self.named(t), self.named(*tycon)) else { return false };
                let variances = self.u.variances(*sym);
                n1.sym == Some(*sym)
                    && self.sub_prefix(n1.prefix, n2.prefix)
                    && self.sub_prefix(n2.prefix, n1.prefix)
                    && args.len() == scrut_args.len()
                    && args
                        .iter()
                        .zip(&scrut_args)
                        .zip(&variances)
                        .all(|((p, &a), &v)| self.match_pattern(p, a, v, widened, inst))
            }
            Pattern::CompileTimeS(arg) => match self.nat_value(scrut) {
                Some(n) if n > 0 => {
                    let pred = self.u.intern_type(Type::Constant(INTconst, n - 1));
                    self.match_pattern(arg, pred, variance, widened, inst)
                }
                _ => false,
            },
        }
    }

    /// The base type of `scrut` for class `c`, a tuple class seen as nested pairs.
    fn match_base_type(&mut self, scrut: TypeId, c: SymbolId) -> Option<TypeId> {
        if let Some(base) = self.u.base_type(scrut, c) {
            return Some(base);
        }
        let widened = self.u.widen(scrut);
        let dealiased = self.u.dealias(widened);
        if dealiased != scrut
            && let Some(base) = self.u.base_type(dealiased, c)
        {
            return Some(base);
        }
        let Type::Applied(tycon, args) = self.u.types().get(dealiased).clone() else { return None };
        let tuple = self.named(tycon)?.sym?;
        let pairs = self.nested_pairs(tuple, &args)?;
        self.u.base_type(pairs, c)
    }

    /// The value of `t` normalized if it is a constant `Int` from 0 (dotty's `natValue`).
    pub fn nat_value(&mut self, t: TypeId) -> Option<i64> {
        let t = self.widen_term_refs(t);
        let t = self.u.dealias(t);
        let t = self.normal(t);
        match *self.u.types().get(t) {
            Type::Constant(INTconst, n) if (0..=i32::MAX as i64).contains(&n) => Some(n),
            _ => None,
        }
    }
}
