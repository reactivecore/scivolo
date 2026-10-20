// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Subtyping (dotty's `TypeComparer`): one comparison for ground types and for types with type
//! variables, whose constraint the caller keeps behind `TypeVars`.

mod applied;
mod atoms;
mod atoms_lub;
mod binders;
mod either;
mod first;
mod fourth;
mod glb;
mod hk;
mod join;
mod lift;
mod lub;
mod match_types;
mod meaning;
mod no_vars;
mod nothing;
mod null;
mod refined;
mod refs;
mod singletons;
mod third;
mod unions;
mod walk;

pub use no_vars::NoVars;
pub use unions::Unions;

use crate::Universe;
use crate::types::{TypeId, VarId};
use rustc_hash::FxHashSet;

/// A state of a constraint to roll back to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mark(pub usize);

/// The constraint on type variables a comparison consults and extends.
pub trait TypeVars {
    /// The type `v` is instantiated to.
    fn instance(&self, v: VarId) -> Option<TypeId>;
    /// Adds `lo <: v`; `false` if that contradicts the constraint.
    fn add_lower(&mut self, u: &mut Universe, v: VarId, lo: TypeId) -> bool;
    /// Adds `v <: hi`; `false` if that contradicts the constraint.
    fn add_upper(&mut self, u: &mut Universe, v: VarId, hi: TypeId) -> bool;
    /// The lower and upper bound of `v`.
    fn bounds(&self, v: VarId) -> (TypeId, TypeId);
    /// Whether a comparison may only consult the constraint.
    fn frozen(&self) -> bool;
    fn mark(&self) -> Mark;
    fn rollback(&mut self, m: Mark);
    /// The changes since `m` that `redo` makes again after a rollback to `m`.
    fn changes_since(&self, m: Mark) -> Changes;
    fn redo(&mut self, changes: Changes);
}

/// Changes to a constraint: the variables whose bounds or instance differ from before, and
/// what the constraint needs to make them again.
pub struct Changes {
    pub vars: Vec<VarId>,
    pub redo: Box<dyn std::any::Any>,
}

impl Universe {
    /// Whether `a` is a subtype of `b`, type variables compared as themselves.
    pub fn is_subtype(&mut self, a: TypeId, b: TypeId) -> bool {
        self.is_subtype_in(a, b, &mut NoVars)
    }

    /// Whether `a` is a subtype of `b` under the constraint `vars`, which the comparison extends
    /// unless it is frozen; a failed comparison leaves `vars` as it was.
    pub fn is_subtype_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool {
        self.compare_in(a, b, vars, false)
    }

    /// `is_subtype_in` keeping of two alternative constraints only one the other subsumes, else
    /// neither (dotty's `necessarySubType`).
    pub fn is_necessary_subtype_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool {
        self.compare_in(a, b, vars, true)
    }

    fn compare_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars, necessary: bool) -> bool {
        let mark = vars.mark();
        let mut c = Cmp::new(self, vars);
        c.necessary = necessary;
        let result = c.recur(a, b);
        if !result {
            vars.rollback(mark);
        }
        result
    }

    /// Whether `a` and `b` are subtypes of each other under the constraint `vars`; a failed
    /// comparison leaves `vars` as it was.
    pub fn is_same_type_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool {
        let mark = vars.mark();
        let mut c = Cmp::new(self, vars);
        let result = c.recur(a, b) && c.recur(b, a);
        if !result {
            vars.rollback(mark);
        }
        result
    }
}

/// How deep a comparison recurses before it watches for a pair it is already comparing.
const WATCH_DEPTH: u32 = 32;
/// How deep a comparison recurses at most; deeper comparisons fail.
const MAX_DEPTH: u32 = 256;

/// A comparison in progress.
pub struct Cmp<'a> {
    pub u: &'a mut Universe,
    pub vars: &'a mut dyn TypeVars,
    frozen: bool,
    /// Whether `either` keeps only necessary constraints.
    necessary: bool,
    depth: u32,
    pending: FxHashSet<(TypeId, TypeId)>,
    /// Whether atoms computed since this was last cleared depend on the constraint, on symbols
    /// or on the depth they were reached at.
    provisional: bool,
    /// How many more steps computing atoms and `Nothing`-ness may take.
    steps: u32,
    /// What the current walk computing atoms or `Nothing`-ness has found.
    walk: walk::Walk,
    /// How many match type reductions are in progress.
    reductions: u32,
}

/// How many steps computing atoms and `Nothing`-ness may take in one comparison.
const MAX_STEPS: u32 = 1_000_000;

impl<'a> Cmp<'a> {
    fn new(u: &'a mut Universe, vars: &'a mut dyn TypeVars) -> Self {
        let (pending, walk) = (FxHashSet::default(), walk::Walk::default());
        Cmp {
            u,
            vars,
            frozen: false,
            necessary: false,
            depth: 0,
            pending,
            provisional: false,
            steps: MAX_STEPS,
            walk,
            reductions: 0,
        }
    }

    /// Takes a step of computing atoms or `Nothing`-ness; false if none is left.
    fn step(&mut self) -> bool {
        self.u.subtype_steps += 1;
        self.steps = self.steps.saturating_sub(1);
        self.steps > 0
    }

    /// Whether the constraint may only be consulted.
    pub fn frozen(&self) -> bool {
        self.frozen || self.vars.frozen()
    }

    /// `a <: b` (dotty's `recur`).
    pub fn recur(&mut self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }
        if self.depth >= MAX_DEPTH {
            return false;
        }
        let watched = self.depth >= WATCH_DEPTH;
        if watched && !self.pending.insert((a, b)) {
            return false;
        }
        self.depth += 1;
        let result = self.first_try(a, b);
        self.depth -= 1;
        if watched {
            self.pending.remove(&(a, b));
        }
        result
    }

    /// `a <: b` without extending the constraint.
    pub fn recur_frozen(&mut self, a: TypeId, b: TypeId) -> bool {
        let frozen = std::mem::replace(&mut self.frozen, true);
        let result = self.recur(a, b);
        self.frozen = frozen;
        result
    }
}
