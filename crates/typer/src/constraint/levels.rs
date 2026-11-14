// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The levels of local symbols and variables (dotty's `nestingLevel`): which are deeper than a
//! variable, deeper variables replaced by ones at its level (dotty's `legalVar`), and bounds that
//! came to refer to local symbols avoided once they go out of scope.

use super::Constraint;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

/// How many variables deep `refers_to_local` looks through bounds for local symbols.
pub const LOCAL_REF_DEPTH: u32 = 4;

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

    /// Gives where a variable made now is made.
    pub fn here(&self, u: &Universe) -> At {
        At { born: u.next_symbol(), nesting: self.nesting }
    }

    /// Replaces each variable without instance in `t` whose bounds refer to a local symbol
    /// defined since `at` by one made `at` with those bounds avoided (dotty's `atLevel` when a
    /// nonvariant variable is interpolated in an enclosing scope); false if a bound cannot do
    /// without them.
    pub fn leveled(&mut self, u: &mut Universe, t: TypeId, at: At) -> bool {
        let mut vars = Vec::new();
        u.map_type(t, &mut |u, part| {
            if let Some(x) = u.var(part) {
                vars.push(x);
            }
            None
        });
        vars.into_iter().all(|x| {
            let e = self.vars[x.0 as usize];
            e.instance.is_some()
                || !(self.refers_to_local(u, e.lo, at, LOCAL_REF_DEPTH)
                    || self.refers_to_local(u, e.hi, at, LOCAL_REF_DEPTH))
                || self.unified_at(u, x, at).is_some()
        })
    }

    /// Makes symbol `s` count as defined where symbol `at` was.
    pub fn define_at(&mut self, s: SymbolId, at: SymbolId) {
        self.defined_at.insert(s, at);
    }

    /// The symbol whose definition `s` counts as.
    fn level(&self, s: SymbolId) -> SymbolId {
        self.defined_at.get(&s).copied().unwrap_or(s)
    }

    /// Whether `v` was made after symbol `s` was defined.
    pub fn made_after(&self, v: VarId, s: SymbolId) -> bool {
        self.vars[v.0 as usize].at.born.0 > self.level(s).0
    }

    /// Whether `t` refers to a local symbol defined since `at`, or has an uninstantiated
    /// variable nested more deeply than `at` whose bounds do, `depth` variables deep.
    pub fn refers_to_local(&self, u: &mut Universe, t: TypeId, at: At, depth: u32) -> bool {
        let mut syms = Vec::new();
        let mut vars = Vec::new();
        u.map_type(t, &mut |u, part| {
            match *u.ty(part) {
                Type::Sym(_, s) => syms.push(s),
                Type::Var(x) => vars.push(x),
                _ => {}
            }
            None
        });
        syms.into_iter().any(|s| self.is_local_since(u, s, at.born))
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
        matches!(u.var(t), Some(x) if self.vars[x.0 as usize].instance.is_none())
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
            if let Some(x) = u.var(part) {
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

    /// Whether `s`, other than a type parameter, is defined from `born` on, or its owner is, in a
    /// method or value.
    pub fn is_local_since(&self, u: &Universe, s: SymbolId, born: SymbolId) -> bool {
        if u.symbol(s).kind == Kind::TypeParam {
            return false;
        }
        let mut s = s;
        while let Some(o) = u.symbol(s).owner {
            if self.level(s).0 >= born.0 && matches!(u.symbol(o).kind, Kind::Def | Kind::Val) {
                return true;
            }
            s = o;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty::{ANY, NOTHING, Symbol};

    fn define(u: &mut Universe, name: &str, kind: Kind, owner: Option<SymbolId>) -> SymbolId {
        u.define(Symbol {
            name: name.into(),
            kind,
            owner,
            flags: Default::default(),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        })
    }

    /// The lower bound a variable bounded below by a value of type `a`, defined after `here`,
    /// gets once leveled to `here`.
    fn leveled_lower_bound(info_of_a: Type) -> (Universe, Type) {
        let (mut c, mut u) = (Constraint::default(), Universe::empty());
        let m = define(&mut u, "m", Kind::Def, None);
        let a = define(&mut u, "a", Kind::Type, Some(m));
        let info = u.intern_type(info_of_a);
        u.set_info(a, info);
        let here = c.here(&u);
        let ok = define(&mut u, "ok", Kind::Val, Some(m));
        let a_ref = u.intern_type(Type::Sym(None, a));
        u.set_info(ok, a_ref);
        let t = c.fresh(&mut u);
        let x = u.var(t).expect("a variable");
        let ok_ref = u.intern_type(Type::Sym(None, ok));
        c.set(x, ok_ref, ANY);
        assert!(c.leveled(&mut u, t, here));
        let kept = c.vars[x.0 as usize].instance.and_then(|i| u.var(i)).expect("a kept variable");
        let lo = *u.ty(c.vars[kept.0 as usize].lo);
        (u, lo)
    }

    #[test]
    fn a_local_value_in_a_bound_is_avoided_through_its_dealiased_type() {
        let (u, lo) = leveled_lower_bound(Type::Alias(ANY));
        assert_eq!(lo, *u.ty(ANY));
    }

    #[test]
    fn a_type_defined_before_the_level_stays_in_the_bound() {
        let (u, lo) = leveled_lower_bound(Type::Bounds(NOTHING, ANY));
        assert!(matches!(lo, Type::Sym(None, a) if u.symbol(a).name == "a"));
    }
}
