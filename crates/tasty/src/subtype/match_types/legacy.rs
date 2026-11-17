// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Matching a case with captures as before Scala 3.4 (dotty's `matchLegacyPatMat`): the
//! scrutinee compared with the pattern, the captures instantiated from the constraint that adds.

use super::super::Cmp;
use super::MAX_REDUCTIONS;
use super::case_vars::CaseVars;
use super::reduce::Outcome;
use super::spec::Legacy;
use crate::types::{NameId, Type, TypeId, VarId};

/// The captures of the legacy case being matched (dotty's `caseLambda`), and how abstract types
/// may widen against them (`canWidenAbstract`, `poisoned`).
pub struct CaseLambda {
    base: u32,
    wildcards: Vec<bool>,
    pub can_widen_abstract: bool,
    poisoned: Vec<bool>,
}

impl CaseLambda {
    /// Whether `v` is one of the captures.
    pub fn owns(&self, v: VarId) -> bool {
        self.index(v).is_some()
    }

    /// Determines the position of `v` among the captures, if it is one.
    pub fn index(&self, v: VarId) -> Option<usize> {
        let i = v.0.checked_sub(self.base)? as usize;
        (i < self.wildcards.len()).then_some(i)
    }

    /// Notes capture `i` as poisoned if it is no wildcard and `variance` is not invariant.
    pub fn poison(&mut self, i: usize, variance: i8) {
        if variance != 0 && !self.wildcards[i] {
            self.poisoned[i] = true;
        }
    }
}

impl Cmp<'_> {
    /// Matches `scrut` against legacy case `case`, its captures type variables over the frozen
    /// constraint (dotty's `matchLegacyPatMat`).
    pub fn match_legacy(&mut self, scrut: TypeId, case: &Legacy) -> Outcome {
        if self.u.reducing >= MAX_REDUCTIONS {
            self.u.reductions_exhausted = true;
            return Outcome::Stuck;
        }
        let base = self.vars.free_var_ids();
        let n = case.params.len();
        let refs: Vec<TypeId> =
            (0..n as u32).map(|i| self.u.intern_type(Type::Var(VarId(base + i)))).collect();
        let names: Vec<NameId> = case.params.iter().map(|&(name, _)| name).collect();
        let pat = self.u.subst(case.pat, case.binder, &names, &refs);
        let body = self.u.subst(case.body, case.binder, &names, &refs);
        let bounds: Vec<(TypeId, TypeId)> = case
            .params
            .iter()
            .map(|&(_, b)| {
                let b = self.u.subst(b, case.binder, &names, &refs);
                match *self.u.ty(b) {
                    Type::Bounds(lo, hi) => (lo, hi),
                    Type::Alias(a) => (a, a),
                    _ => (crate::NOTHING, crate::ANY),
                }
            })
            .collect();
        let wildcards = names
            .iter()
            .map(|&name| {
                let s = self.u.name(name);
                s == "_" || s.starts_with("_$")
            })
            .collect();

        let mut vars = CaseVars::new(&mut *self.vars, base, &bounds);
        let mut c = Cmp::new(self.u, &mut vars);
        c.frozen = true;
        c.depth = self.depth;
        c.normalizing = self.normalizing.clone();
        c.legacy = Some(CaseLambda { base, wildcards, can_widen_abstract: false, poisoned: vec![false; n] });
        c.legacy_outcome(scrut, pat, body)
    }

    fn legacy_outcome(&mut self, scrut: TypeId, pat: TypeId, body: TypeId) -> Outcome {
        let disjoint = self.provably_disjoint(scrut, pat);
        let reduced = if self.legacy_matches(scrut, pat, false) {
            self.redux(pat, body, true)
        } else if self.legacy_matches(scrut, pat, true) {
            self.redux(pat, body, false)
        } else if disjoint {
            return Outcome::Disjoint;
        } else {
            return Outcome::Stuck;
        };
        match reduced {
            Some(t) if !disjoint => Outcome::Reduced(t),
            _ => Outcome::Stuck,
        }
    }

    /// Whether `scrut <: pat` holds, abstract types widened against variant captures only if
    /// `can_widen_abstract` (dotty's `matches`); the captures it poisons noted.
    fn legacy_matches(&mut self, scrut: TypeId, pat: TypeId, can_widen_abstract: bool) -> bool {
        if let Some(l) = self.legacy.as_mut() {
            l.can_widen_abstract = can_widen_abstract;
            l.poisoned.fill(false);
        }
        let (mark, necessary) = (self.vars.mark(), std::mem::replace(&mut self.necessary, true));
        let matched = self.recur(scrut, pat);
        self.necessary = necessary;
        if !matched {
            self.vars.rollback(mark);
        }
        matched
    }

    /// `body` with the captures instantiated (dotty's `redux`): from the bound their variance
    /// in `pat` picks if `can_approx`, else to their lower bound if they are not poisoned and
    /// it is their upper one; `None` if a capture `body` is has no instance. Refuses a capture
    /// without instance elsewhere in `body`, which dotty approximates, and an instance that
    /// mentions captures.
    fn redux(&mut self, pat: TypeId, body: TypeId, can_approx: bool) -> Option<TypeId> {
        let l = self.legacy.as_ref()?;
        let (base, n, poisoned) = (l.base, l.wildcards.len(), l.poisoned.clone());
        let mut occurrences = Vec::new();
        self.capture_variances(pat, 1, &mut occurrences);
        let mut instances: Vec<Option<TypeId>> = vec![None; n];
        for (i, variance) in occurrences {
            let (lo, hi) = self.vars.bounds(VarId(base + i as u32));
            instances[i] = match can_approx {
                true if variance >= 0 || self.mentions_captures(hi) => Some(lo),
                true => Some(hi),
                false if !poisoned[i] && self.recur(hi, lo) => Some(lo),
                false => None,
            };
        }
        if instances.iter().flatten().any(|&t| self.mentions_captures(t)) {
            self.u.reductions_exhausted = true;
            return None;
        }

        if let Some(i) = self.u.var(body).and_then(|v| self.legacy.as_ref()?.index(v)) {
            return instances[i];
        }
        let mut refused = false;
        let reduced = self.u.map_type(body, &mut |u, t| {
            let i = u.var(t).filter(|v| v.0 >= base).map(|v| (v.0 - base) as usize).filter(|&i| i < n)?;
            refused |= instances[i].is_none();
            Some(instances[i].unwrap_or(t))
        });
        if refused {
            self.u.reductions_exhausted = true;
            return None;
        }
        Some(reduced)
    }

    /// Whether `t` mentions a capture of the legacy case being matched.
    fn mentions_captures(&mut self, t: TypeId) -> bool {
        let mut found = Vec::new();
        self.capture_variances(t, 1, &mut found);
        !found.is_empty()
    }
}
