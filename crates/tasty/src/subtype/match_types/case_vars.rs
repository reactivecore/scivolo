// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The constraint of a legacy match case: its captures as type variables beside the constraint
//! of the comparison, which stays as it is (dotty's `constrained(caseLambda)`).

use super::super::{Changes, Mark, TypeVars};
use crate::Universe;
use crate::types::{TypeId, VarId};

/// The id of the first capture variable of a case matched outside other cases, above the ids
/// of the typer's constraint.
pub const CASE_VARS: u32 = 1 << 30;

#[derive(Clone, Copy)]
struct Entry {
    lo: TypeId,
    hi: TypeId,
}

/// The bounds of the captures of one case, variables `base..`, over the constraint `outer`.
pub struct CaseVars<'v> {
    outer: &'v mut dyn TypeVars,
    base: u32,
    entries: Vec<Entry>,
    /// Each change: the capture and its bounds before.
    undo: Vec<(usize, Entry)>,
}

impl<'v> CaseVars<'v> {
    /// Creates the captures `base..` with the bounds `bounds`, over `outer`.
    pub fn new(outer: &'v mut dyn TypeVars, base: u32, bounds: &[(TypeId, TypeId)]) -> Self {
        let entries = bounds.iter().map(|&(lo, hi)| Entry { lo, hi }).collect();
        CaseVars { outer, base, entries, undo: Vec::new() }
    }

    fn own(&self, v: VarId) -> Option<usize> {
        let i = v.0.checked_sub(self.base)? as usize;
        (i < self.entries.len()).then_some(i)
    }

    fn set(&mut self, i: usize, e: Entry) {
        self.undo.push((i, self.entries[i]));
        self.entries[i] = e;
    }

    /// Sets the bounds of capture `i` to `e` if `e.lo <: e.hi` holds, the other captures
    /// constrained as that needs.
    fn narrow(&mut self, u: &mut Universe, i: usize, e: Entry) -> bool {
        let mark = self.mark();
        self.set(i, e);
        let ok = u.is_subtype_in(e.lo, e.hi, self);
        if !ok {
            self.rollback(mark);
        }
        ok
    }
}

impl TypeVars for CaseVars<'_> {
    fn instance(&self, v: VarId) -> Option<TypeId> {
        match self.own(v) {
            Some(_) => None,
            None => self.outer.instance(v),
        }
    }

    fn add_lower(&mut self, u: &mut Universe, v: VarId, lo: TypeId) -> bool {
        let Some(i) = self.own(v) else { return false };
        let e = self.entries[i];
        if u.var(lo) == Some(v) || u.is_subtype_frozen_in(lo, e.lo, self) {
            return true;
        }
        let lo = match u.is_subtype_frozen_in(e.lo, lo, self) {
            true => lo,
            false => u.lub(e.lo, lo),
        };
        self.narrow(u, i, Entry { lo, ..e })
    }

    fn add_upper(&mut self, u: &mut Universe, v: VarId, hi: TypeId) -> bool {
        let Some(i) = self.own(v) else { return false };
        let e = self.entries[i];
        if u.var(hi) == Some(v) || u.is_subtype_frozen_in(e.hi, hi, self) {
            return true;
        }
        let hi = match u.is_subtype_frozen_in(hi, e.hi, self) {
            true => hi,
            false => u.glb(e.hi, hi),
        };
        self.narrow(u, i, Entry { hi, ..e })
    }

    fn bounds(&self, v: VarId) -> (TypeId, TypeId) {
        match self.own(v) {
            Some(i) => (self.entries[i].lo, self.entries[i].hi),
            None => self.outer.bounds(v),
        }
    }

    fn frozen(&self) -> bool {
        false
    }

    fn mark(&self) -> Mark {
        Mark(self.undo.len())
    }

    fn rollback(&mut self, m: Mark) {
        while self.undo.len() > m.0 {
            let Some((i, e)) = self.undo.pop() else { break };
            self.entries[i] = e;
        }
    }

    fn changes_since(&self, m: Mark) -> Changes {
        let mut changed: Vec<usize> = self.undo[m.0..].iter().map(|&(i, _)| i).collect();
        changed.sort_unstable();
        changed.dedup();
        let vars = changed.iter().map(|&i| VarId(self.base + i as u32)).collect();
        let redo: Vec<(usize, Entry)> = changed.into_iter().map(|i| (i, self.entries[i])).collect();
        Changes { vars, redo: Box::new(redo) }
    }

    fn redo(&mut self, changes: Changes) {
        let Ok(redo) = changes.redo.downcast::<Vec<(usize, Entry)>>() else { return };
        for (i, e) in *redo {
            self.set(i, e);
        }
    }

    fn free_var_ids(&self) -> u32 {
        self.base + self.entries.len() as u32
    }
}
