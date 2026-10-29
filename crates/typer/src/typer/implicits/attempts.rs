//! Implicit searches and candidates tried speculatively: the constraint, the recorded fallback
//! and the dictionary of by-name implicits restored afterwards.

use super::super::Typer;
use super::candidates::Candidate;
use super::history::STALE;
use super::search::is_misfit;
use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::TypeId;

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
            _ if !clean => Err("nested implicit search outside the subset"),
            _ if linked => Err(STALE),
            Ok(_) => Ok(true),
            Err(what) if is_misfit(what) => Ok(false),
            Err(what) => Err(what),
        }
    }

    /// The candidates of `group` that `attempt` accepts, each tried with the constraint, the
    /// recorded fallback and the dictionary restored afterwards; `Err` if one fails other than by
    /// not fitting, or if the dictionary depends on which of several candidates dotty tries.
    pub fn fitting(
        &mut self,
        group: &[Candidate],
        mut attempt: impl FnMut(&mut Self, Candidate) -> Result<TNodeId, &'static str>,
    ) -> Result<Vec<Candidate>, &'static str> {
        let mut found = Vec::new();
        let mut linked_any = false;
        for &c in group {
            if matches!(self.syms.completions.get(&c.sym), Some(Completion::Completing(_))) {
                return Err("implicit candidate whose definition is being completed");
            }
            let (result, clean, linked) = self.speculatively(|t| attempt(t, c));
            if !clean {
                return Err("implicit candidate outside the subset");
            }
            match result {
                Ok(_) => {
                    found.push(c);
                    linked_any |= linked;
                }
                Err(what) if is_misfit(what) && linked => return Err(STALE),
                Err(what) if is_misfit(what) => {}
                Err(what) => return Err(what),
            }
        }
        if linked_any && found.len() > 1 {
            return Err(STALE);
        }
        Ok(found)
    }

    /// The candidate of `group`, those whose types fit `pt`, that ranks above all others, tried
    /// and kept if it succeeds, the others skipped (dotty's `rank`); not tried if `unpruned`.
    pub fn best_first(
        &mut self,
        group: &[Candidate],
        (pt, by_name): (TypeId, bool),
        at: Span,
        ctx: Ctx,
        depth: u32,
        unpruned: bool,
    ) -> Result<BestFirst, &'static str> {
        let Some(best) = (!unpruned).then(|| self.most_specific(group, ctx)).flatten() else {
            return Ok(BestFirst::Skipped);
        };
        let (before, unsupported) = (self.constraint.attempt(), self.unsupported.clone());
        let dictionary = self.history.snapshot();
        let result = self.tried(best, (pt, by_name), at, ctx, (depth, false));
        let clean = self.unsupported == unsupported;
        let linked = self.history.changed(&dictionary);
        let outcome = match result {
            _ if !clean => Err("implicit candidate outside the subset"),
            Ok(tree) if !linked || group.len() == 1 => return Ok(BestFirst::Found(tree)),
            Ok(_) => Ok(BestFirst::Skipped),
            Err(what) if is_misfit(what) && linked => Err(STALE),
            Err(what) if is_misfit(what) => Ok(BestFirst::Failed),
            Err(what) => Err(what),
        };
        self.constraint.abandon(before);
        self.unsupported = unsupported;
        self.history.restore(dictionary);
        outcome
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
        self.unsupported = unsupported;
        self.history.restore(dictionary);
        (result, clean, linked)
    }
}
