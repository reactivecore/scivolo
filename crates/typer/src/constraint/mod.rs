// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//   compiler/src/dotty/tools/dotc/core/OrderingConstraint.scala
//! The bounds and instances of the type variables of a unit (dotty's `OrderingConstraint`,
//! without the ordering between variables), with an undo log for `either` and failed
//! comparisons and abandoned attempts.

mod attempt;
mod avoid;
mod unify;

use attempt::Fate;
use tasty::{ANY, Changes, Mark, NOTHING, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

#[derive(Clone, Copy, PartialEq)]
struct Entry {
    lo: TypeId,
    hi: TypeId,
    instance: Option<TypeId>,
    /// Whether a hard union was added as a lower bound (dotty's hard variable).
    hard: bool,
    /// The first symbol defined after the variable was made; the later, the deeper its level.
    born: SymbolId,
}

#[derive(Default)]
pub struct Constraint {
    vars: Vec<Entry>,
    /// The fate of each variable, not undone by `rollback`.
    fates: Vec<Fate>,
    /// The entries changed since a mark, their previous states.
    log: Vec<(usize, Entry)>,
    /// How many satisfiability checks are in progress, one inside another.
    checks: u32,
    /// Whether a bound is being propagated from another variable.
    propagating: bool,
    /// The number of variables when each implicit argument search in progress started.
    searches: Vec<usize>,
    /// Whether a lower bound referred to a local symbol defined after its variable and could not
    /// do without it, also in an abandoned attempt.
    escaped: bool,
}

/// How many satisfiability checks may be in progress at once; deeper ones fail.
const MAX_CHECKS: u32 = 8;

impl Constraint {
    /// A fresh variable without bounds, of any kind until it gets an upper bound other than `Any`.
    pub fn fresh(&mut self, u: &mut Universe) -> TypeId {
        let born = u.next_symbol();
        self.fresh_at(u, born)
    }

    /// `fresh` at the level of the variables made when symbol `born` was the next one.
    fn fresh_at(&mut self, u: &mut Universe, born: SymbolId) -> TypeId {
        self.vars.push(Entry { lo: NOTHING, hi: ANY, instance: None, hard: false, born });
        self.fates.push(Fate::Open);
        u.intern_type(Type::Var(VarId(self.vars.len() as u32 - 1)))
    }

    /// How many variables there are; the ones created later have higher ids.
    pub fn count(&self) -> usize {
        self.vars.len()
    }

    /// Instantiates `v` to `t`.
    pub fn instantiate(&mut self, v: VarId, t: TypeId) {
        let i = v.0 as usize;
        self.log.push((i, self.vars[i]));
        self.vars[i].instance = Some(t);
    }

    fn set(&mut self, v: VarId, lo: TypeId, hi: TypeId) {
        let i = v.0 as usize;
        self.log.push((i, self.vars[i]));
        self.vars[i].lo = lo;
        self.vars[i].hi = hi;
    }

    /// Starts an implicit argument search, also one nested in another.
    pub fn enter_search(&mut self) {
        self.searches.push(self.vars.len());
    }

    pub fn leave_search(&mut self) {
        self.searches.pop();
    }

    /// Whether `v` and `w` may be unified: outside searches, or in an outermost search one made
    /// by it and one made before it.
    fn unifiable(&self, v: VarId, w: VarId) -> bool {
        match self.searches[..] {
            [] => true,
            [m] => (v.0 as usize >= m) != (w.0 as usize >= m),
            _ => false,
        }
    }

    /// Whether a local symbol escaped into the bound of a variable made before it.
    pub fn escaped(&self) -> bool {
        self.escaped
    }

    /// Whether `v` keeps the unions of its lower bound when instantiated (dotty's `isHard`).
    pub fn is_hard(&self, v: VarId) -> bool {
        self.vars[v.0 as usize].hard
    }

    /// Adds bound `t` to variable `w` that bounds another variable, a lower bound if `lower`
    /// (dotty's propagation along the ordering of variables); false if unsatisfiable.
    fn propagated(&mut self, u: &mut Universe, w: VarId, t: TypeId, lower: bool) -> bool {
        if self.checks >= MAX_CHECKS {
            return false;
        }
        self.checks += 1;
        let propagating = std::mem::replace(&mut self.propagating, true);
        let ok = if lower { self.add_lower(u, w, t) } else { self.add_upper(u, w, t) };
        self.propagating = propagating;
        self.checks -= 1;
        ok
    }

    /// Whether `lo <: hi` holds, extending the constraint.
    fn satisfiable(&mut self, u: &mut Universe, lo: TypeId, hi: TypeId) -> bool {
        if self.checks >= MAX_CHECKS {
            return false;
        }
        self.checks += 1;
        let ok = u.is_subtype_in(lo, hi, self);
        self.checks -= 1;
        ok
    }

    /// Propagates bound `t` of `v`, a lower one if `lower`, to the variable `other` it bounds, if
    /// one, then narrows `v` to `lo` and `hi` as `narrow` does; false and unchanged if either fails.
    fn add_bound(
        &mut self,
        u: &mut Universe,
        v: VarId,
        (t, lower): (TypeId, bool),
        other: TypeId,
        (lo, hi): (TypeId, TypeId),
        checked: TypeId,
    ) -> bool {
        let mark = self.mark();
        let propagated = match *u.types().get(other) {
            Type::Var(w) => self.propagated(u, w, t, lower),
            _ => true,
        };
        let ok = propagated && self.narrow(u, v, lo, hi, checked);
        if !ok {
            self.rollback(mark);
        }
        ok
    }

    /// Sets the bounds of `v` to `lo` and `hi` if `checked <: hi` holds then, `checked` being
    /// the part of `lo` to check (dotty's `addOneBound`).
    fn narrow(&mut self, u: &mut Universe, v: VarId, lo: TypeId, hi: TypeId, checked: TypeId) -> bool {
        let mark = self.mark();
        self.set(v, lo, hi);
        let ok = hi == ANY || self.satisfiable(u, checked, hi);
        if !ok {
            self.rollback(mark);
        }
        ok
    }
}

impl TypeVars for Constraint {
    fn instance(&self, v: VarId) -> Option<TypeId> {
        self.vars[v.0 as usize].instance
    }

    fn add_lower(&mut self, u: &mut Universe, v: VarId, lo: TypeId) -> bool {
        let e = self.vars[v.0 as usize];
        if let Some(t) = e.instance {
            return self.satisfiable(u, lo, t);
        }
        if *u.types().get(lo) == Type::Var(v) {
            return true;
        }
        let lo = match self.avoided(u, lo, e.born, 1) {
            Some(avoided) => avoided,
            None => {
                self.escaped = true;
                lo
            }
        };
        let incoming_union = matches!(u.types().get(lo), Type::Or(_, _, false));
        let (lo, unchecked) = if let Some(joined) = u.atoms_lub(e.lo, lo) {
            (joined, lo)
        } else if u.is_subtype(lo, e.lo) {
            (e.lo, e.lo)
        } else if u.is_subtype(e.lo, lo) {
            (lo, lo)
        } else {
            let lub = u.lub(e.lo, lo);
            (lub, lub)
        };
        if !self.add_bound(u, v, (lo, true), e.hi, (lo, e.hi), unchecked) {
            return false;
        }
        if incoming_union && !self.propagating {
            self.vars[v.0 as usize].hard = true;
        }
        true
    }

    fn add_upper(&mut self, u: &mut Universe, v: VarId, hi: TypeId) -> bool {
        let e = self.vars[v.0 as usize];
        if let Some(t) = e.instance {
            return self.satisfiable(u, t, hi);
        }
        if *u.types().get(hi) == Type::Var(v) {
            return true;
        }
        if let Type::Var(w) = *u.types().get(hi)
            && self.unifiable(v, w)
            && self.unify(u, v, w)
        {
            return true;
        }
        let hi = if e.hi == ANY {
            hi
        } else if u.is_subtype(e.hi, hi) {
            e.hi
        } else if u.is_subtype(hi, e.hi) {
            hi
        } else {
            u.glb(e.hi, hi)
        };
        self.add_bound(u, v, (hi, false), e.lo, (e.lo, hi), e.lo)
    }

    fn bounds(&self, v: VarId) -> (TypeId, TypeId) {
        let e = self.vars[v.0 as usize];
        (e.lo, e.hi)
    }

    fn frozen(&self) -> bool {
        false
    }

    fn mark(&self) -> Mark {
        Mark(self.log.len())
    }

    fn rollback(&mut self, m: Mark) {
        while self.log.len() > m.0 {
            let (i, e) = self.log.pop().expect("a logged entry");
            self.vars[i] = e;
        }
    }

    fn changes_since(&self, m: Mark) -> Changes {
        let mut before: Vec<(usize, Entry)> = Vec::new();
        for &(i, e) in &self.log[m.0..] {
            if !before.iter().any(|&(j, _)| j == i) {
                before.push((i, e));
            }
        }
        let now: Vec<(usize, Entry)> =
            before.into_iter().filter(|&(i, e)| self.vars[i] != e).map(|(i, _)| (i, self.vars[i])).collect();
        Changes { vars: now.iter().map(|&(i, _)| VarId(i as u32)).collect(), redo: Box::new(now) }
    }

    fn redo(&mut self, changes: Changes) {
        let Ok(entries) = changes.redo.downcast::<Vec<(usize, Entry)>>() else {
            unreachable!("changes of this constraint")
        };
        for (i, e) in *entries {
            self.log.push((i, self.vars[i]));
            self.vars[i] = e;
        }
    }
}
