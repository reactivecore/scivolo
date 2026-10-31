// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The levels of local symbols and variables (dotty's `nestingLevel`): which are deeper than a
//! variable, deeper variables replaced by ones at its level (dotty's `legalVar`), and bounds that
//! came to refer to local symbols avoided once they go out of scope.

use super::Constraint;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

impl Constraint {
    /// Whether `t` refers to a local symbol defined from `born` on, or has an uninstantiated
    /// variable made after `born` whose bounds do, `depth` variables deep.
    pub fn refers_to_local(&self, u: &mut Universe, t: TypeId, born: SymbolId, depth: u32) -> bool {
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
        syms.into_iter().any(|s| is_local_since(u, s, born))
            || vars.into_iter().any(|x| {
                let e = self.vars[x.0 as usize];
                let bounds = |c: &Self, u: &mut Universe, b| c.refers_to_local(u, b, born, depth - 1);
                match e.instance {
                    Some(i) => depth == 0 || bounds(self, u, i),
                    None if e.born.0 <= born.0 => false,
                    None => depth == 0 || bounds(self, u, e.lo) || bounds(self, u, e.hi),
                }
            })
    }

    /// Whether `t` is a variable without instance.
    pub fn is_open_var(&self, u: &Universe, t: TypeId) -> bool {
        matches!(*u.types().get(t), Type::Var(x) if self.vars[x.0 as usize].instance.is_none())
    }

    /// A fresh variable at the level of `born`, `x` instantiated to it, its bounds those of `x`
    /// without the local symbols defined from `born` on; `None` if they cannot do without them
    /// (dotty's `legalVar` at variance 0).
    pub fn unified_at(&mut self, u: &mut Universe, x: VarId, born: SymbolId) -> Option<TypeId> {
        let e = self.vars[x.0 as usize];
        let mark = self.mark();
        let kept = self.fresh_at(u, born);
        let k = VarId(self.vars.len() as u32 - 1);
        self.instantiate(x, kept);
        let (Some(lo), Some(hi)) = (self.avoided(u, e.lo, born, 1), self.avoided(u, e.hi, born, -1)) else {
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
            let (Some(lo), Some(hi)) = (self.avoided(u, e.lo, e.born, 1), self.avoided(u, e.hi, e.born, -1))
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
