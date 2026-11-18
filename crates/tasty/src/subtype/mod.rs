// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Subtyping (dotty's `TypeComparer`): one comparison for ground types and for types with type
//! variables, whose constraint the caller keeps behind `TypeVars`.

mod applied;
mod applied_gadt;
mod args;
mod atoms;
mod atoms_lub;
mod binders;
mod class_symbols;
mod curried;
mod either;
mod eta;
mod first;
mod fourth;
mod gadt_narrow;
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
mod refined_members;
mod refs;
mod singletons;
mod third;
mod unions;
mod walk;

pub use match_types::MatchTypeRules;
pub use no_vars::NoVars;
pub use null::NullMode;
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
    /// The first variable id a constraint nested in this one may take.
    fn free_var_ids(&self) -> u32 {
        match_types::CASE_VARS
    }
}

/// Changes to a constraint: the variables whose bounds or instance differ from before, and
/// what the constraint needs to make them again.
pub struct Changes {
    pub vars: Vec<VarId>,
    pub redo: Box<dyn std::any::Any>,
}

impl Universe {
    /// Whether `a` is a subtype of `b`, type variables compared as themselves and the GADT
    /// constraint only consulted.
    pub fn is_subtype(&mut self, a: TypeId, b: TypeId) -> bool {
        let inferring = std::mem::replace(&mut self.gadt_mut().inferring, false);
        let result = self.is_subtype_in(a, b, &mut NoVars);
        self.gadt_mut().inferring = inferring;
        result
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

    /// Whether `a <: b` holds under the constraint `vars` as it is (dotty's `isSubTypeWhenFrozen`).
    pub fn is_subtype_frozen_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool {
        Cmp::new(self, vars).recur_frozen(a, b)
    }

    fn compare_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars, necessary: bool) -> bool {
        let mark = vars.mark();
        let mut c = Cmp::new(self, vars);
        c.necessary = necessary;
        let result = c.recur(a, b);
        drop(c);
        if !result {
            vars.rollback(mark);
        }
        result
    }

    /// Like `is_necessary_subtype_in`, narrowing the GADT bounds of the symbols registered in
    /// the GADT constraint where the comparison needs it (dotty's comparison in
    /// `Mode.GadtConstraintInference`); a failed comparison leaves `vars` and the GADT constraint
    /// as they were.
    pub fn constrain_gadt_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool {
        let (mark, gadt_mark) = (vars.mark(), self.gadt().mark());
        let inferring = std::mem::replace(&mut self.gadt_mut().inferring, true);
        let mut c = Cmp::new(self, vars);
        c.necessary = true;
        let result = c.recur(a, b);
        drop(c);
        self.gadt_mut().inferring = inferring;
        if !result {
            vars.rollback(mark);
            self.gadt_mut().rollback(gadt_mark);
        }
        result
    }

    /// Whether `a` and `b` are subtypes of each other under `vars`, unchanged if they are not.
    pub fn is_same_type_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool {
        let mark = vars.mark();
        let mut c = Cmp::new(self, vars);
        let result = c.recur(a, b) && c.recur(b, a);
        drop(c);
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
    /// The match types being normalized, outermost first.
    normalizing: Vec<TypeId>,
    /// Whether the comparison narrows the GADT constraint (dotty's `GadtConstraintInference`).
    gadt_mode: bool,
    /// Whether the GADT constraint may only be consulted (dotty's `frozenGadt`).
    frozen_gadt: bool,
    /// Which bounds of an abstract type the current pair was reached through (dotty's `approx`).
    approx: gadt_narrow::Approx,
    /// The legacy match case whose captures the comparison constrains (dotty's `caseLambda`).
    legacy: Option<match_types::CaseLambda>,
}

/// How many steps computing atoms and `Nothing`-ness may take in one comparison.
const MAX_STEPS: u32 = 1_000_000;

impl<'a> Cmp<'a> {
    fn new(u: &'a mut Universe, vars: &'a mut dyn TypeVars) -> Self {
        let inferring = u.gadt().inferring;
        Cmp {
            u,
            vars,
            frozen: false,
            necessary: inferring,
            depth: 0,
            pending: FxHashSet::default(),
            provisional: false,
            steps: MAX_STEPS,
            walk: walk::Walk::default(),
            reductions: 0,
            normalizing: Vec::new(),
            gadt_mode: inferring,
            frozen_gadt: false,
            approx: gadt_narrow::Approx::default(),
            legacy: None,
        }
    }

    /// Whether the comparison may constrain variable `v` (dotty's `canConstrain`).
    fn can_constrain(&self, v: VarId) -> bool {
        !self.frozen() || self.legacy.as_ref().is_some_and(|l| l.owns(v))
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
