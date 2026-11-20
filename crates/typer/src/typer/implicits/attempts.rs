//! Implicit searches and candidates tried speculatively: the constraint, the recorded fallback
//! and the dictionary of by-name implicits restored afterwards.

use super::super::Typer;
use super::candidates::Candidate;
use super::history::STALE;
use super::ranking::most_specific;
use super::search::{ImplicitSearch, is_misfit};
use super::unsettled::has_unbounded_var;
use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::TypeId;

/// The refusal of a candidate whose attempt met a construct outside the subset.
pub const CANDIDATE_OUTSIDE: &str = "implicit candidate outside the subset";
/// The refusal of a nested search that met a construct outside the subset.
pub const NESTED_OUTSIDE: &str = "nested implicit search outside the subset";

/// What trying the best candidate of a group first gave.
pub enum BestFirst {
    Found(TNodeId),
    /// It does not fit.
    Failed,
    /// Not tried, or its success not decisive.
    Skipped,
}

impl Typer<'_> {
    /// Whether an implicit argument of type `pt` exists at `at` in `ctx`, the constraint unchanged.
    pub fn implicit_exists(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<bool, &'static str> {
        let (result, clean, linked) = self.speculatively(|t| t.implicit_arg(pt, at, ctx, depth));
        match result {
            _ if !clean => Err(NESTED_OUTSIDE),
            _ if linked => Err(STALE),
            Ok(_) => Ok(true),
            Err(what) if is_misfit(what) => Ok(false),
            Err(what) => Err(what),
        }
    }

    /// The result of `f` run with the constraint, the recorded fallback and the dictionary
    /// restored afterwards; whether it recorded no fallback, and whether it changed the dictionary.
    pub fn speculatively<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> (T, bool, bool) {
        let (before, unsupported) = (self.constraint.attempt(), self.unsupported.clone());
        let dictionary = self.history.snapshot();
        let result = f(self);
        let clean = self.unsupported == unsupported;
        let linked = self.history.changed(&dictionary);
        self.constraint.abandon(before);
        self.restore_unsupported(unsupported);
        self.history.restore(dictionary);
        (result, clean, linked)
    }

    /// Restores the recorded fallback to `before`, keeping one recorded since as the undone one.
    pub fn restore_unsupported(&mut self, before: Option<(Span, String)>) {
        if self.unsupported != before
            && let Some((_, what)) = std::mem::replace(&mut self.unsupported, before)
        {
            self.undone_unsupported = Some(what);
        }
    }
}

/// Completes the definition of candidate `c` outside any attempt on it (dotty's
/// `completeInCreationContext`); `Err` if it is being completed.
pub fn complete_candidate(t: &mut Typer<'_>, c: Candidate) -> Result<(), &'static str> {
    if matches!(t.syms.completions.get(&c.sym), Some(Completion::Completing(_))) {
        return Err("implicit candidate whose definition is being completed");
    }
    t.complete(c.sym);
    Ok(())
}

impl ImplicitSearch {
    /// The candidate of `group`, those whose types fit, that ranks above all others, tried and
    /// kept if it succeeds, the others skipped (dotty's `rank`); not tried if `unpruned`.
    pub fn best_first(
        &self,
        t: &mut Typer<'_>,
        group: &[Candidate],
        unpruned: bool,
    ) -> Result<BestFirst, &'static str> {
        let Some(best) = (!unpruned).then(|| most_specific(t, group, self.ctx)).flatten() else {
            return Ok(BestFirst::Skipped);
        };
        complete_candidate(t, best)?;
        let (before, unsupported) = (t.constraint.attempt(), t.unsupported.clone());
        let dictionary = t.history.snapshot();
        let result = self.tried(t, best, false);
        let clean = t.unsupported == unsupported;
        let linked = t.history.changed(&dictionary);
        let outcome = match result {
            _ if !clean => Err(CANDIDATE_OUTSIDE),
            Ok(tree) if !linked || group.len() == 1 => return Ok(BestFirst::Found(tree)),
            Ok(_) => Ok(BestFirst::Skipped),
            Err(what) if is_misfit(what) && linked => Err(STALE),
            Err(what) if is_misfit(what) => Ok(BestFirst::Failed),
            Err(what) => Err(what),
        };
        t.constraint.abandon(before);
        t.restore_unsupported(unsupported);
        t.history.restore(dictionary);
        outcome
    }

    /// The first candidate of `group` that succeeds, its changes to the constraint kept, the
    /// others before it skipped (dotty's `rank` of a coherent search); `None` if none fits.
    pub fn first_fitting(
        &self,
        t: &mut Typer<'_>,
        group: &[Candidate],
    ) -> Result<Option<TNodeId>, &'static str> {
        let mut nested_failure = false;
        for &c in group {
            complete_candidate(t, c)?;
            let (before, unsupported) = (t.constraint.attempt(), t.unsupported.clone());
            let dictionary = t.history.snapshot();
            let result = self.tried(t, c, false);
            let clean = t.unsupported == unsupported;
            let linked = t.history.changed(&dictionary);
            let outcome = match result {
                _ if !clean => Err(CANDIDATE_OUTSIDE),
                Ok(_) if nested_failure && has_unbounded_var(&mut t.types(), self.pt) => {
                    Err("implicit candidate rejected by a search with type variables")
                }
                Ok(tree) => return Ok(Some(tree)),
                Err(what) if is_misfit(what) && linked => Err(STALE),
                Err(what) if is_misfit(what) => {
                    nested_failure |= what == "no implicit argument";
                    Ok(())
                }
                Err(what) => Err(what),
            };
            t.constraint.abandon(before);
            t.restore_unsupported(unsupported);
            t.history.restore(dictionary);
            outcome?;
        }
        Ok(None)
    }

    /// Whether the candidates `found` give arguments of one type.
    pub fn found_one_type(&self, t: &mut Typer<'_>, found: &[Candidate]) -> Result<bool, &'static str> {
        let mut types = Vec::new();
        for &c in found {
            let (tpe, _, _) = t.speculatively(|t| {
                let tree = self.tried(t, c, false)?;
                let tpe = t.tpd.tpe(tree);
                Ok::<_, &'static str>(t.types().zonk(tpe))
            });
            types.push(tpe?);
        }
        Ok(types.windows(2).all(|w| w[0] == w[1]))
    }
}
