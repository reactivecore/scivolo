// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TyperState.scala
//! Attempts given up: their changes undone and the type variables they made discarded (dotty's
//! uncommitted `TyperState` dropped with its `ownedVars`).

use super::Constraint;
use tasty::{Mark, TypeVars, VarId};

/// The state of a constraint before an attempt.
#[derive(Clone, Copy)]
pub struct Attempt {
    mark: Mark,
    vars: usize,
}

/// Whether a variable is discarded with an abandoned attempt, kept from that, or neither yet.
#[derive(Clone, Copy, PartialEq)]
pub enum Fate {
    Open,
    Kept,
    Discarded,
}

impl Constraint {
    pub fn attempt(&self) -> Attempt {
        Attempt { mark: self.mark(), vars: self.vars.len() }
    }

    /// Undoes the changes since `a` and discards the variables made since, but kept ones.
    pub fn abandon(&mut self, a: Attempt) {
        self.rollback(a.mark);
        self.settle_from(a.vars, Fate::Discarded);
    }

    /// Keeps the variables of id `from` or later from being discarded.
    pub fn keep_from(&mut self, from: usize) {
        self.settle_from(from, Fate::Kept);
    }

    fn settle_from(&mut self, from: usize, fate: Fate) {
        for f in self.fates[from..].iter_mut().filter(|f| **f == Fate::Open) {
            *f = fate;
        }
    }

    /// Whether `v` is neither instantiated nor made by an abandoned attempt.
    pub fn is_pending(&self, v: VarId) -> bool {
        self.instance(v).is_none() && self.fates[v.0 as usize] != Fate::Discarded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty::{Type, Universe};

    fn var(c: &mut Constraint, u: &mut Universe) -> VarId {
        let t = c.fresh(u);
        let Type::Var(v) = *u.types().get(t) else { unreachable!() };
        v
    }

    #[test]
    fn an_abandoned_attempt_leaves_no_pending_variable() {
        let (mut c, mut u) = (Constraint::default(), Universe::empty());
        let outer = var(&mut c, &mut u);
        let a = c.attempt();
        let inner = var(&mut c, &mut u);
        c.instantiate(outer, tasty::ANY);
        c.abandon(a);
        assert!(c.is_pending(outer));
        assert!(!c.is_pending(inner));
    }

    #[test]
    fn kept_variables_survive_an_abandoned_attempt() {
        let (mut c, mut u) = (Constraint::default(), Universe::empty());
        let a = c.attempt();
        let nested = c.attempt();
        let dropped = var(&mut c, &mut u);
        c.abandon(nested);
        let kept = var(&mut c, &mut u);
        c.keep_from(dropped.0 as usize);
        c.abandon(a);
        assert!(!c.is_pending(dropped));
        assert!(c.is_pending(kept));
    }
}
