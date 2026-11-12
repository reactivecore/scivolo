// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The GADT bounds in force: set by the cases being typed, undone for a definition typed ahead
//! outside them.

use super::Typer;
use super::type_ops::TypeOps;
use crate::ctx::{LevelId, Levels};
use rustc_hash::FxHashSet;
use tasty::{SymbolId, Type, TypeId, Universe};

/// Holds what the cases being typed know of the type parameters their patterns narrow: in
/// `def f[T](b: Box[T]): T = b match { case IntBox(i) => i }` the case knows `T = Int` (dotty's
/// GADT constraint in the `Context`). One field of `Typer`, set on entering a case and undone on
/// leaving it (ADR 0114).
#[derive(Default)]
pub struct Gadt {
    /// Each narrowed symbol with its GADT bounds before, innermost last.
    saved: Vec<(SymbolId, Option<(TypeId, TypeId)>)>,
    /// The scope level of each case being typed, with the length of `saved` at its start.
    open: Vec<(LevelId, usize)>,
    /// The pattern-bound types of the cases being typed.
    pub bound: Vec<SymbolId>,
    /// How many cases are being typed.
    pub cases: u32,
    /// Whether the innermost case being typed is one of an inline match.
    pub inline: bool,
    /// The owners given to the functions of polymorphic function values, whose type parameters
    /// a case inside them may narrow.
    pub poly_owners: FxHashSet<SymbolId>,
}

/// The state of `Gadt` before a case, restored after it.
pub struct CaseMark {
    saved: usize,
    bound: usize,
    open: usize,
    cases: u32,
    inline: bool,
}

/// The GADT bounds undone while a definition is typed ahead, restored after it.
pub struct Suspended {
    undone: Vec<(SymbolId, Option<(TypeId, TypeId)>)>,
    cases: u32,
}

impl Typer<'_> {
    /// Runs `f` as the typing of a case, of an inline match if `inline`, and restores the bounds
    /// its pattern narrows after it.
    pub fn in_gadt_case<R>(&mut self, inline: bool, f: impl FnOnce(&mut Self) -> R) -> R {
        let mark = self.gadt.enter_case(inline);
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
    /// Enters the typing of a case, of an inline match if `inline`.
    pub fn enter_case(&mut self, inline: bool) -> CaseMark {
        let mark = CaseMark {
            saved: self.saved.len(),
            bound: self.bound.len(),
            open: self.open.len(),
            cases: self.cases,
            inline: self.inline,
        };
        (self.cases, self.inline) = (mark.cases + 1, inline);
        mark
    }

    /// Leaves the case entered at `mark`, giving the types its pattern narrowed their bounds
    /// before.
    pub fn leave_case(&mut self, u: &mut Universe, mark: CaseMark) {
        (self.cases, self.inline) = (mark.cases, mark.inline);
        self.bound.truncate(mark.bound);
        self.open.truncate(mark.open);
        for (s, bounds) in self.saved.drain(mark.saved..).rev() {
            u.set_gadt_bounds(s, bounds);
        }
    }

    /// Undoes, for a definition typed ahead at `level`, the bounds of the cases that do not
    /// enclose it.
    pub fn suspend(&mut self, u: &mut Universe, levels: &Levels, level: LevelId) -> Suspended {
        let first = self.open.iter().find(|&&(case, _)| !encloses(levels, case, level));
        let mark = first.map_or(self.saved.len(), |&(_, mark)| mark);
        let undone =
            self.saved[mark..].iter().rev().map(|&(s, old)| (s, u.set_gadt_bounds(s, old))).collect();
        Suspended { undone, cases: std::mem::replace(&mut self.cases, 0) }
    }

    /// Restores the bounds `suspend` undid.
    pub fn resume(&mut self, u: &mut Universe, suspended: Suspended) {
        self.cases = suspended.cases;
        for (s, bounds) in suspended.undone.into_iter().rev() {
            u.set_gadt_bounds(s, bounds);
        }
    }

    /// Records `level` as the scope of the case being typed, before its pattern.
    pub fn case_level(&mut self, level: LevelId) {
        self.open.push((level, self.saved.len()));
    }

    /// Adds the types a pattern binds, `syms`, to those a case may narrow.
    pub fn bind(&mut self, syms: &[SymbolId]) {
        if self.cases > 0 {
            self.bound.extend(syms);
        }
    }

    /// Gives the pattern-bound types from the `first`th on their GADT bounds as infos (dotty's
    /// `indexPattern`).
    pub fn index_binds(&self, u: &mut Universe, first: usize) {
        for &s in &self.bound[first..] {
            let Some((lo, hi)) = u.gadt_bounds(s) else { continue };
            let info = u.intern_type(Type::Bounds(lo, hi));
            u.set_info(s, info);
        }
    }

    /// Narrows `s` to `bounds` while the case is typed.
    pub fn narrow(&mut self, u: &mut Universe, s: SymbolId, (lo, hi): (TypeId, TypeId)) {
        let old = u.set_gadt_bounds(s, Some((lo, hi)));
        self.saved.push((s, old));
    }

    /// Undoes the bounds narrowed after the first `mark` and gives each type narrowed since with
    /// its bounds before undoing, in narrowing order.
    pub fn undo_since(&mut self, u: &mut Universe, mark: usize) -> Vec<(SymbolId, Option<(TypeId, TypeId)>)> {
        let now = self.saved[mark..].iter().map(|&(s, _)| (s, u.gadt_bounds(s))).collect();
        for (s, bounds) in self.saved.drain(mark..).rev() {
            u.set_gadt_bounds(s, bounds);
        }
        now
    }

    /// Tells whether `a` conforms to `b` without the GADT bounds, constraining type variables.
    pub fn conforms_without(&self, ops: &mut TypeOps, a: TypeId, b: TypeId) -> bool {
        let narrowed = self.narrowed();
        let current: Vec<_> = narrowed.iter().map(|&s| (s, ops.u.set_gadt_bounds(s, None))).collect();
        let ok = ops.conforms(a, b);
        for (s, bounds) in current.into_iter().rev() {
            ops.u.set_gadt_bounds(s, bounds);
        }
        ok
    }

    /// Tells whether GADT bounds are in force.
    pub fn narrowing(&self) -> bool {
        !self.saved.is_empty()
    }

    /// Lists the types GADT bounds narrow.
    pub fn narrowed(&self) -> Vec<SymbolId> {
        self.saved.iter().map(|&(s, _)| s).collect()
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
