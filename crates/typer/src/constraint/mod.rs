//! The bounds and instances of the type variables of a unit (dotty's `OrderingConstraint`,
//! without the ordering between variables), with an undo log for `either` and failed
//! comparisons and abandoned attempts.

mod attempt;

use attempt::Fate;
use tasty::{ANY, Mark, NOTHING, Type, TypeId, TypeVars, Universe, VarId};

#[derive(Clone, Copy)]
struct Entry {
    lo: TypeId,
    hi: TypeId,
    instance: Option<TypeId>,
    /// Whether a hard union was added as a lower bound (dotty's hard variable).
    hard: bool,
}

#[derive(Default)]
pub struct Constraint {
    vars: Vec<Entry>,
    /// The fate of each variable, not undone by `rollback`.
    fates: Vec<Fate>,
    /// The entries changed since a mark, their previous states.
    log: Vec<(usize, Entry)>,
    frozen: bool,
    /// How many satisfiability checks are in progress, one inside another.
    checks: u32,
    /// Whether a bound is being propagated from another variable.
    propagating: bool,
}

/// How many satisfiability checks may be in progress at once; deeper ones fail.
const MAX_CHECKS: u32 = 8;

impl Constraint {
    /// A fresh variable without bounds, of any kind until it gets an upper bound other than `Any`.
    pub fn fresh(&mut self, u: &mut Universe) -> TypeId {
        self.vars.push(Entry { lo: NOTHING, hi: ANY, instance: None, hard: false });
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

    /// Whether `lo <: hi` holds without extending the constraint.
    fn satisfiable(&mut self, u: &mut Universe, lo: TypeId, hi: TypeId) -> bool {
        if self.checks >= MAX_CHECKS {
            return false;
        }
        self.checks += 1;
        let frozen = std::mem::replace(&mut self.frozen, true);
        let ok = u.is_subtype_in(lo, hi, self);
        self.frozen = frozen;
        self.checks -= 1;
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
        if let Type::Var(w) = *u.types().get(e.hi)
            && !self.propagated(u, w, lo, true)
        {
            return false;
        }
        if e.hi != ANY && !self.satisfiable(u, unchecked, e.hi) {
            return false;
        }
        self.set(v, lo, e.hi);
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
        let hi = if e.hi == ANY {
            hi
        } else if u.is_subtype(e.hi, hi) {
            e.hi
        } else if u.is_subtype(hi, e.hi) {
            hi
        } else {
            u.glb(e.hi, hi)
        };
        if let Type::Var(w) = *u.types().get(e.lo)
            && !self.propagated(u, w, hi, false)
        {
            return false;
        }
        if !self.satisfiable(u, e.lo, hi) {
            return false;
        }
        self.set(v, e.lo, hi);
        true
    }

    fn bounds(&self, v: VarId) -> (TypeId, TypeId) {
        let e = self.vars[v.0 as usize];
        (e.lo, e.hi)
    }

    fn frozen(&self) -> bool {
        self.frozen
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
}
