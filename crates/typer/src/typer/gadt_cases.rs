// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The GADT bounds in force: set by the cases being typed, undone for a definition typed ahead
//! outside them.

use super::Typer;
use super::type_ops::TypeOps;
use crate::ctx::{LevelId, Levels};
use rustc_hash::FxHashSet;
use tasty::{GadtChanges, Mark, SymbolId, Type, TypeId, Universe};

/// Holds what the cases being typed know of the type parameters their patterns narrow: in
/// `def f[T](b: Box[T]): T = b match { case IntBox(i) => i }` the case knows `T = Int` (dotty's
/// GADT constraint in the `Context`), kept in the universe's GADT constraint and rolled back on
/// leaving the case (ADR 0114, 0119).
#[derive(Default)]
pub struct Gadt {
    /// The scope level of each case being typed, with the GADT constraint's mark at its start.
    open: Vec<(LevelId, Mark)>,
    /// How many definitions typed ahead have GADT bounds undone.
    suspended: usize,
    /// The pattern-bound types of the cases being typed.
    pub bound: Vec<SymbolId>,
    /// How many cases are being typed.
    pub cases: u32,
    /// The owners given to the functions of polymorphic function values, whose type parameters
    /// a case inside them may narrow.
    pub poly_owners: FxHashSet<SymbolId>,
}

/// The state of `Gadt` before a case, restored after it.
pub struct CaseMark {
    store: Mark,
    bound: usize,
    open: usize,
    cases: u32,
}

/// The GADT bounds undone while a definition is typed ahead, restored after it.
pub struct Suspended {
    undone: Option<GadtChanges>,
    cases: u32,
}

impl Typer<'_> {
    /// Runs `f` as the typing of a case and restores the bounds its pattern narrows after it.
    pub fn in_gadt_case<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let mark = self.gadt.enter_case(self.u);
        let result = f(self);
        self.gadt.leave_case(self.u, mark);
        result
    }

    /// Runs `f` as the typing ahead of a definition at `level`: without the bounds of the cases
    /// that do not enclose it, and narrowing nothing outside a case of its own (dotty's completer
    /// context).
    pub fn outside_gadt_cases<R>(&mut self, level: LevelId, f: impl FnOnce(&mut Self) -> R) -> R {
        let suspended = self.gadt.suspend(self.u, &self.levels, level);
        let result = f(self);
        self.gadt.resume(self.u, suspended);
        result
    }
}

impl Gadt {
    /// Enters the typing of a case.
    pub fn enter_case(&mut self, u: &Universe) -> CaseMark {
        let mark = CaseMark {
            store: u.gadt().mark(),
            bound: self.bound.len(),
            open: self.open.len(),
            cases: self.cases,
        };
        self.cases = mark.cases + 1;
        mark
    }

    /// Leaves the case entered at `mark`, giving the types its pattern narrowed their bounds
    /// before.
    pub fn leave_case(&mut self, u: &mut Universe, mark: CaseMark) {
        self.cases = mark.cases;
        self.bound.truncate(mark.bound);
        self.open.truncate(mark.open);
        u.gadt_mut().rollback(mark.store);
    }

    /// Undoes, for a definition typed ahead at `level`, the bounds of the cases that do not
    /// enclose it.
    pub fn suspend(&mut self, u: &mut Universe, levels: &Levels, level: LevelId) -> Suspended {
        let first = self.open.iter().find(|&&(case, _)| !encloses(levels, case, level));
        let mark = first.map_or(u.gadt().mark(), |&(_, mark)| mark);
        let undone = u.gadt().changes_since(mark);
        u.gadt_mut().rollback(mark);
        self.suspended += usize::from(undone.is_some());
        Suspended { undone, cases: std::mem::replace(&mut self.cases, 0) }
    }

    /// Restores the bounds `suspend` undid.
    pub fn resume(&mut self, u: &mut Universe, suspended: Suspended) {
        self.cases = suspended.cases;
        if let Some(changes) = suspended.undone {
            self.suspended -= 1;
            u.gadt_mut().redo(changes);
        }
    }

    /// Records `level` as the scope of the case being typed, before its pattern.
    pub fn case_level(&mut self, u: &Universe, level: LevelId) {
        self.open.push((level, u.gadt().mark()));
    }

    /// Adds the types a pattern binds, `syms`, to those a case may narrow.
    pub fn bind(&mut self, syms: &[SymbolId]) {
        if self.cases > 0 {
            self.bound.extend(syms);
        }
    }

    /// Gives the pattern-bound types from the `first`th on their GADT bounds as infos, an alias
    /// for one made equal to a type (dotty's `indexPattern`).
    pub fn index_binds(&self, u: &mut Universe, first: usize) {
        for &s in &self.bound[first..] {
            let Some((lo, hi)) = u.gadt_full_bounds(s) else { continue };
            let info = match u.gadt().entry(s).and_then(|e| e.instance) {
                Some(i) => Type::Alias(i),
                None => Type::Bounds(lo, hi),
            };
            let info = u.intern_type(info);
            u.set_info(s, info);
        }
    }

    /// Narrows `s` to `bounds` while the case is typed.
    pub fn narrow(&mut self, u: &mut Universe, s: SymbolId, (lo, hi): (TypeId, TypeId)) {
        u.set_gadt_bounds(s, Some((lo, hi)));
    }

    /// Undoes the GADT constraint's changes after `mark` and gives them for `redo`.
    pub fn undo_since(&mut self, u: &mut Universe, mark: Mark) -> Option<GadtChanges> {
        let changes = u.gadt().changes_since(mark);
        u.gadt_mut().rollback(mark);
        changes
    }

    /// Tells whether `a` conforms to `b` against an empty GADT constraint, constraining type
    /// variables.
    pub fn conforms_without(&self, ops: &mut TypeOps, a: TypeId, b: TypeId) -> bool {
        let gadt = std::mem::take(ops.u.gadt_mut());
        let ok = ops.conforms(a, b);
        *ops.u.gadt_mut() = gadt;
        ok
    }

    /// Tells whether GADT bounds are in force, also those undone for a definition typed ahead.
    pub fn narrowing(&self, u: &Universe) -> bool {
        u.has_gadt_bounds() || self.suspended > 0
    }
}

/// Tells whether scope level `case` is `level` or encloses it.
fn encloses(levels: &Levels, case: LevelId, level: LevelId) -> bool {
    let mut l = Some(level);
    while let Some(i) = l {
        if i == case {
            return true;
        }
        l = levels.outer(i);
    }
    false
}
