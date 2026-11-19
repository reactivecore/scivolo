//! Match types that stay unreduced as dotty keeps them, by a case whose pattern tests for a class
//! that strictly derives from the scrutinee's class.

use super::super::{Cmp, NoVars};
use super::MAX_TOTAL_REDUCTIONS;
use super::reduce::Outcome;
use super::rules::MatchTypeRules;
use super::spec::{Pattern, Spec};
use crate::types::{Type, TypeId};
use crate::{SymbolId, Universe};

/// How deeply nested class tests `stuck_at` follows.
const MAX_TYPE_TESTS: u32 = 32;

impl Universe {
    /// Whether match type `t` does not reduce for certain: its cases before one are disjoint from
    /// the scrutinee, which neither matches that one nor is disjoint from it since the pattern's
    /// class strictly derives from the scrutinee's, as `Tuple.Zip[Tuple, Tuple]` at
    /// `case (h1 *: t1, h2 *: t2)`. Only under the rules from Scala 3.4.
    pub fn certainly_stuck(&mut self, t: TypeId) -> bool {
        Cmp::new(self, &mut NoVars).certainly_stuck(t)
    }
}

impl Cmp<'_> {
    fn certainly_stuck(&mut self, t: TypeId) -> bool {
        if self.u.match_type_rules != MatchTypeRules::Spec {
            return false;
        }
        let Some(m) = self.underlying_normalizable(t) else { return false };
        let Type::Match { scrutinee, cases, .. } = *self.u.ty(m) else { return false };
        if self.u.reducing == 0 {
            self.u.reductions_left = MAX_TOTAL_REDUCTIONS;
        }
        let exhausted = std::mem::take(&mut self.u.reductions_exhausted);
        self.u.reducing += 1;
        let frozen = std::mem::replace(&mut self.frozen, true);

        let scrutinee = self.u.dealias(scrutinee);
        let scrutinee = self.fixed(scrutinee);
        let scrut = self.normal(scrutinee);
        let cases = self.u.types_of(cases).to_vec();
        let mut stuck = false;
        for case in cases {
            let (outcome, pattern) = match self.analyze(case) {
                Spec::Specced(s) => {
                    let bounded = self.u.subst(s.pat, s.binder, &s.names, &s.bounds);
                    (self.match_specced(scrut, &s), Some((s.pattern, bounded)))
                }
                Spec::SubTypeTest(pat, body) => (self.match_sub_type_test(scrut, pat, body), None),
                Spec::Legacy(_) | Spec::Unsupported => break,
            };
            match (outcome, pattern) {
                (Outcome::Disjoint, _) => continue,
                (Outcome::Stuck, Some((pattern, pat))) => {
                    stuck = self.stuck_at(scrut, &pattern, pat, MAX_TYPE_TESTS)
                }
                _ => {}
            }
            break;
        }

        self.frozen = frozen;
        self.u.reducing -= 1;
        let refused = std::mem::replace(&mut self.u.reductions_exhausted, exhausted);
        stuck && !refused
    }

    /// Whether `scrut` neither matches `pattern`, of type `pat` with its captures bounded, nor is
    /// disjoint from it: `pattern` tests for a class strictly deriving from `scrut`'s class
    /// without type parameters, or for `scrut`'s class with covariant parameters, each argument
    /// such a test, a capture or a pattern it conforms to, one such a test.
    fn stuck_at(&mut self, scrut: TypeId, pattern: &Pattern, pat: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let Pattern::BaseTypeTest { class, args, .. } = pattern else { return false };
        let Some((c, scrut_args)) = self.class_parts(scrut, *class) else { return false };
        if c != *class {
            return scrut_args.is_empty()
                && self.u.type_params(c).is_empty()
                && self.u.derives_from(*class, c);
        }
        let Some((pc, pat_args)) = self.class_parts(pat, *class) else { return false };
        let covariant = self.u.variances(c).iter().all(|&v| v == 1);
        if pc != c || !covariant || scrut_args.len() != args.len() || pat_args.len() != args.len() {
            return false;
        }

        let mut tested = false;
        for ((&a, p), &pt) in scrut_args.iter().zip(args).zip(&pat_args) {
            match p {
                Pattern::Capture(..) => {}
                Pattern::BaseTypeTest { .. } if self.stuck_at(a, p, pt, depth) => tested = true,
                Pattern::BaseTypeTest { .. } | Pattern::TypeTest if self.recur(a, pt) => {}
                _ => return false,
            }
        }
        tested
    }

    /// The class `t` normalized is a reference to and its type arguments; a tuple class as nested
    /// pairs if `class` is `*:`.
    fn class_parts(&mut self, t: TypeId, class: SymbolId) -> Option<(SymbolId, Vec<TypeId>)> {
        let t = self.normal(t);
        let t = self.u.dealias(t);
        let (tycon, args) = match self.u.applied(t) {
            Some((tycon, a)) => (tycon, a.to_vec()),
            None => (t, Vec::new()),
        };
        let c = self.named(tycon)?.sym.filter(|&c| self.u.is_class(c))?;
        if c != class
            && let Some(pairs) = self.nested_pairs(c, &args)
        {
            return self.class_parts(pairs, class);
        }
        Some((c, args))
    }
}
