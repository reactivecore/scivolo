// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The GADT bounds in force: set by the cases being typed, undone for a definition typed ahead
//! outside them.

use super::Typer;
use crate::ctx::LevelId;
use tasty::{SymbolId, TypeId};

/// The type parameters narrowed by the patterns of the cases being typed.
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
}

impl Typer<'_> {
    /// `f` as the typing of a case: the bounds its pattern narrows restored after.
    pub fn in_gadt_case<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let (mark, bound, open) = (self.gadt.saved.len(), self.gadt.bound.len(), self.gadt.open.len());
        let cases = self.gadt.cases;
        self.gadt.cases = cases + 1;
        let result = f(self);
        self.gadt.cases = cases;
        self.gadt.bound.truncate(bound);
        self.gadt.open.truncate(open);
        let narrowed: Vec<_> = self.gadt.saved.drain(mark..).rev().collect();
        for (s, bounds) in narrowed {
            self.u.set_gadt_bounds(s, bounds);
        }
        result
    }

    /// Records `level` as the scope of the case being typed, before its pattern.
    pub fn gadt_case_level(&mut self, level: LevelId) {
        self.gadt.open.push((level, self.gadt.saved.len()));
    }

    /// `f` as the typing ahead of a definition at `level`: without the bounds of the cases that do
    /// not enclose it, and narrowing nothing outside a case of its own (dotty's completer context).
    pub fn outside_gadt_cases<R>(&mut self, level: LevelId, f: impl FnOnce(&mut Self) -> R) -> R {
        let first = self.gadt.open.iter().find(|&&(case, _)| !self.level_encloses(case, level));
        let mark = first.map_or(self.gadt.saved.len(), |&(_, mark)| mark);
        let undone: Vec<_> = self.gadt.saved[mark..]
            .iter()
            .rev()
            .map(|&(s, old)| (s, self.u.set_gadt_bounds(s, old)))
            .collect();
        let cases = std::mem::replace(&mut self.gadt.cases, 0);
        let result = f(self);
        self.gadt.cases = cases;
        for (s, bounds) in undone.into_iter().rev() {
            self.u.set_gadt_bounds(s, bounds);
        }
        result
    }

    /// Whether scope level `case` is `level` or encloses it.
    fn level_encloses(&self, case: LevelId, level: LevelId) -> bool {
        let mut l = Some(level);
        while let Some(i) = l {
            if i == case {
                return true;
            }
            l = self.levels.outer(i);
        }
        false
    }

    /// Narrows `s` to `bounds` while the case is typed.
    pub fn set_gadt_bounds(&mut self, s: SymbolId, (lo, hi): (TypeId, TypeId)) {
        let old = self.u.set_gadt_bounds(s, Some((lo, hi)));
        self.gadt.saved.push((s, old));
    }

    /// Whether `a` conforms to `b` without the GADT bounds, constraining type variables.
    pub fn conforms_without_gadt(&mut self, a: TypeId, b: TypeId) -> bool {
        let narrowed = self.gadt_narrowed();
        let current: Vec<_> = narrowed.iter().map(|&s| (s, self.u.set_gadt_bounds(s, None))).collect();
        let ok = self.conforms(a, b);
        for (s, bounds) in current.into_iter().rev() {
            self.u.set_gadt_bounds(s, bounds);
        }
        ok
    }

    /// Whether GADT bounds are in force.
    pub fn gadt_narrowing(&self) -> bool {
        !self.gadt.saved.is_empty()
    }

    /// The types GADT bounds narrow.
    pub fn gadt_narrowed(&self) -> Vec<SymbolId> {
        self.gadt.saved.iter().map(|&(s, _)| s).collect()
    }
}
