//! The bounds and instances of the type variables of a unit (dotty's `OrderingConstraint`,
//! without the ordering between variables), with an undo log for `either` and failed
//! comparisons.

use tasty::{ANY, Mark, NOTHING, Type, TypeId, TypeVars, Universe, VarId};

#[derive(Clone, Copy)]
struct Entry {
    lo: TypeId,
    hi: TypeId,
    instance: Option<TypeId>,
}

#[derive(Default)]
pub struct Constraint {
    vars: Vec<Entry>,
    /// The entries changed since a mark, their previous states.
    log: Vec<(usize, Entry)>,
    frozen: bool,
    /// How many satisfiability checks are in progress, one inside another.
    checks: u32,
}

/// How many satisfiability checks may be in progress at once; deeper ones fail.
const MAX_CHECKS: u32 = 8;

/// The constants of `t` if it is a constant or a union of constants (dotty's `Atoms`).
fn atoms(u: &Universe, t: TypeId, out: &mut Vec<TypeId>) -> bool {
    match *u.types().get(t) {
        Type::Or(a, b) => atoms(u, a, out) && atoms(u, b, out),
        Type::Constant(..) | Type::Str(_) => {
            out.push(t);
            true
        }
        _ => false,
    }
}

/// The union of `parts`, balanced (dotty's `OrType.balanced`).
fn balanced_union(u: &mut Universe, parts: &[TypeId]) -> TypeId {
    if let [t] = parts {
        return *t;
    }
    let (a, b) = parts.split_at(parts.len() / 2);
    let (a, b) = (balanced_union(u, a), balanced_union(u, b));
    u.intern_type(Type::Or(a, b))
}

/// The least upper bound of `a` and `b` if both are constants or unions of constants.
fn atoms_lub(u: &mut Universe, a: TypeId, b: TypeId) -> Option<TypeId> {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    if !atoms(u, a, &mut xs) || !atoms(u, b, &mut ys) {
        return None;
    }
    if ys.iter().all(|y| xs.contains(y)) {
        return Some(a);
    }
    if xs.iter().all(|x| ys.contains(x)) {
        return Some(b);
    }
    if ys.iter().any(|y| xs.contains(y)) {
        return None;
    }
    let left = xs.len();
    xs.extend(ys);
    Some(if left.abs_diff(xs.len() - left) <= 1 {
        u.intern_type(Type::Or(a, b))
    } else {
        balanced_union(u, &xs)
    })
}

impl Constraint {
    /// A fresh variable without bounds.
    pub fn fresh(&mut self, u: &mut Universe) -> TypeId {
        self.vars.push(Entry { lo: NOTHING, hi: ANY, instance: None });
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

    /// Adds bound `t` to variable `w` that bounds another variable, a lower bound if `lower`
    /// (dotty's propagation along the ordering of variables); false if unsatisfiable.
    fn propagated(&mut self, u: &mut Universe, w: VarId, t: TypeId, lower: bool) -> bool {
        if self.checks >= MAX_CHECKS {
            return false;
        }
        self.checks += 1;
        let ok = if lower { self.add_lower(u, w, t) } else { self.add_upper(u, w, t) };
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
        let lo = if let Some(joined) = atoms_lub(u, e.lo, lo) {
            joined
        } else if u.is_subtype(lo, e.lo) {
            e.lo
        } else if u.is_subtype(e.lo, lo) {
            lo
        } else {
            u.lub(e.lo, lo)
        };
        if let Type::Var(w) = *u.types().get(e.hi)
            && !self.propagated(u, w, lo, true)
        {
            return false;
        }
        if !self.satisfiable(u, lo, e.hi) {
            return false;
        }
        self.set(v, lo, e.hi);
        true
    }

    fn add_upper(&mut self, u: &mut Universe, v: VarId, hi: TypeId) -> bool {
        let e = self.vars[v.0 as usize];
        if let Some(t) = e.instance {
            return self.satisfiable(u, t, hi);
        }
        let hi = if u.is_subtype(e.hi, hi) {
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
