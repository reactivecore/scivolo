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

impl Typer<'_> {
    /// Whether an implicit argument of type `pt` exists at `at` in `ctx`, the constraint unchanged.
    pub fn implicit_exists(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<bool, &'static str> {
        let (before, unsupported) = (self.constraint.attempt(), self.unsupported.clone());
        let dictionary = self.history.snapshot();
        let result = self.implicit_arg(pt, at, ctx, depth);
        let clean = self.unsupported == unsupported;
        let linked = self.history.lazies != dictionary.0;
        self.constraint.abandon(before);
        self.unsupported = unsupported;
        self.history.restore(dictionary);
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
            let (before, unsupported) = (self.constraint.attempt(), self.unsupported.clone());
            let dictionary = self.history.snapshot();
            let result = attempt(self, c);
            let clean = self.unsupported == unsupported;
            let linked = self.history.lazies != dictionary.0;
            self.constraint.abandon(before);
            self.unsupported = unsupported;
            self.history.restore(dictionary);
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
}
