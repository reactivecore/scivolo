// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The levels of local symbols and variables (dotty's `nestingLevel`): which are deeper than a
//! variable, deeper variables replaced by ones at its level (dotty's `legalVar`), and bounds that
//! came to refer to local symbols avoided once they go out of scope.

use super::Constraint;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

/// Where a variable was made: the first symbol defined after it, and the nesting of the scope it
/// was made in (dotty's `nestingLevel`).
#[derive(Clone, Copy, PartialEq)]
pub struct At {
    pub born: SymbolId,
    pub nesting: u32,
}

impl Constraint {
    /// Sets the nesting of the scope being typed, returning the previous one.
    pub fn set_nesting(&mut self, nesting: u32) -> u32 {
        std::mem::replace(&mut self.nesting, nesting)
    }

    /// Whether `v` was made after symbol `s` was defined.
    pub fn made_after(&self, v: VarId, s: SymbolId) -> bool {
        self.vars[v.0 as usize].at.born.0 > s.0
    }

    /// Whether `t` refers to a local symbol defined since `at`, or has an uninstantiated
    /// variable nested more deeply than `at` whose bounds do, `depth` variables deep.
    pub fn refers_to_local(&self, u: &mut Universe, t: TypeId, at: At, depth: u32) -> bool {
        let mut syms = Vec::new();
        let mut vars = Vec::new();
        u.map_type(t, &mut |u, part| {
            match *u.types().get(part) {
                Type::Sym(_, s) => syms.push(s),
                Type::Var(x) => vars.push(x),
                _ => {}
            }
            None
        });
        syms.into_iter().any(|s| is_local_since(u, s, at.born))
            || vars.into_iter().any(|x| {
                let e = self.vars[x.0 as usize];
                let bounds = |c: &Self, u: &mut Universe, b| c.refers_to_local(u, b, at, depth - 1);
                match e.instance {
                    Some(i) => depth == 0 || bounds(self, u, i),
                    None if e.at.nesting <= at.nesting => false,
                    None => depth == 0 || bounds(self, u, e.lo) || bounds(self, u, e.hi),
                }
            })
    }

    /// Whether `t` is a variable without instance.
    pub fn is_open_var(&self, u: &Universe, t: TypeId) -> bool {
        matches!(*u.types().get(t), Type::Var(x) if self.vars[x.0 as usize].instance.is_none())
    }

    /// A fresh variable made `at`, `x` instantiated to it, its bounds those of `x` without the
    /// local symbols defined since `at`; `None` if they cannot do without them (dotty's
    /// `legalVar` at variance 0).
    pub fn unified_at(&mut self, u: &mut Universe, x: VarId, at: At) -> Option<TypeId> {
        let e = self.vars[x.0 as usize];
        let mark = self.mark();
        let kept = self.fresh_at(u, at);
        let k = VarId(self.vars.len() as u32 - 1);
        self.instantiate(x, kept);
        let (Some(lo), Some(hi)) = (self.avoided(u, e.lo, at, 1), self.avoided(u, e.hi, at, -1)) else {
            self.rollback(mark);
            return None;
        };
        self.set(k, lo, hi);
        self.vars[k.0 as usize].hard = e.hard;
        Some(kept)
    }

    /// Avoids, in the bounds of each variable without instance in `t`, the local symbols
    /// defined after the variable, which they came to refer to through variables made later;
    /// false if a bound cannot do without them.
    pub fn avoid_in_bounds(&mut self, u: &mut Universe, t: TypeId) -> bool {
        let mut vars = Vec::new();
        u.map_type(t, &mut |u, part| {
            if let Type::Var(x) = *u.types().get(part) {
                vars.push(x);
            }
            None
        });
        for x in vars {
            let e = self.vars[x.0 as usize];
            if let Some(i) = e.instance {
                if !self.avoid_in_bounds(u, i) {
                    return false;
                }
                continue;
            }
            let (Some(lo), Some(hi)) = (self.avoided(u, e.lo, e.at, 1), self.avoided(u, e.hi, e.at, -1))
            else {
                return false;
            };
            if (lo, hi) != (e.lo, e.hi) {
                self.set(x, lo, hi);
            }
        }
        true
    }
}

/// Whether `s`, other than a type parameter, is defined from `born` on, or its owner is, in a
/// method or value.
pub fn is_local_since(u: &Universe, s: SymbolId, born: SymbolId) -> bool {
    if u.symbol(s).kind == Kind::TypeParam {
        return false;
    }
    let mut s = s;
    while let Some(o) = u.symbol(s).owner {
        if s.0 >= born.0 && matches!(u.symbol(o).kind, Kind::Def | Kind::Val) {
            return true;
        }
        s = o;
    }
    false
}
