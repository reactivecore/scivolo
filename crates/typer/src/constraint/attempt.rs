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

    /// Keeps the variables made since `a` from being discarded, and their changes since `a` from
    /// being undone; changes to older variables stay undoable.
    pub fn keep(&mut self, a: Attempt) {
        self.settle_from(a.vars, Fate::Kept);
        let older: Vec<_> = self.log.drain(a.mark.0..).filter(|&(i, _)| i < a.vars).collect();
        self.log.extend(older);
    }

    fn settle_from(&mut self, from: usize, fate: Fate) {
        for (i, f) in self.fates.iter_mut().enumerate().skip(from).filter(|(_, f)| **f == Fate::Open) {
            *f = fate;
            if fate == Fate::Discarded {
                self.pending.remove(&(i as u32));
            }
        }
    }

    /// Whether `v` is neither instantiated nor made by an abandoned attempt.
    pub fn is_pending(&self, v: VarId) -> bool {
        self.instance(v).is_none() && self.fates[v.0 as usize] != Fate::Discarded
    }

    /// Lists the pending variables of id `from` or later, by id.
    pub fn pending_from(&self, from: usize) -> impl Iterator<Item = VarId> + '_ {
        debug_assert!(
            self.pending
                .iter()
                .copied()
                .eq((0..self.vars.len() as u32).filter(|&i| self.is_pending(VarId(i))))
        );
        self.pending.range(from as u32..).map(|&i| VarId(i))
    }

    /// Updates the pending set after entry `i` changed.
    pub fn sync_pending(&mut self, i: usize) {
        if self.is_pending(VarId(i as u32)) {
            self.pending.insert(i as u32);
        } else {
            self.pending.remove(&(i as u32));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty::Universe;

    fn var(c: &mut Constraint, u: &mut Universe) -> VarId {
        let t = c.fresh(u);
        let Some(v) = u.var(t) else { unreachable!() };
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
        assert_eq!(c.pending_from(0).collect::<Vec<_>>(), [outer]);
    }

    #[test]
    fn kept_variables_and_their_instances_survive_an_abandoned_attempt() {
        let (mut c, mut u) = (Constraint::default(), Universe::empty());
        let outer = var(&mut c, &mut u);
        let a = c.attempt();
        let completion = c.attempt();
        let nested = c.attempt();
        let dropped = var(&mut c, &mut u);
        c.abandon(nested);
        let kept = var(&mut c, &mut u);
        let instantiated = var(&mut c, &mut u);
        c.instantiate(instantiated, tasty::ANY);
        c.instantiate(outer, tasty::ANY);
        c.keep(completion);
        c.abandon(a);
        assert!(!c.is_pending(dropped));
        assert!(c.is_pending(kept));
        assert_eq!(c.instance(instantiated), Some(tasty::ANY));
        assert!(c.is_pending(outer));
        assert_eq!(c.pending_from(0).collect::<Vec<_>>(), [outer, kept]);
    }
}
