//! The singleton types a type consists of (dotty's `Atoms`), and comparisons decided by them
//! (`compareAtoms`).
//! Not modelled: the object as atom of its module class, the bounds of binder-bound type
//! parameters, match type reduction, the declared bound of a type variable without instance
//! (its atoms are unknown).

use super::refs::Meaning;
use super::{Cmp, MAX_DEPTH};
use crate::NOTHING;
use crate::types::{Type, TypeId};
use rustc_hash::FxHashSet;

/// The singletons a type must (`lo`) and may (`hi`) consist of, as lists with repeats; dotty's
/// `Atoms.Range`, where `None` stands for `Atoms.Unknown`.
#[derive(Default)]
pub struct Range {
    pub lo: Vec<TypeId>,
    pub hi: Vec<TypeId>,
}

impl Range {
    /// The atoms of a type only known to be among `self.hi`.
    fn upper(self) -> Range {
        Range { lo: Vec::new(), hi: self.hi }
    }
}

impl Cmp<'_> {
    /// `a <: b` by atoms if `b`'s are known: true if `a` may only have atoms `b` must have, or if
    /// `known_singletons` and both are the same single atom; false if `a` must have one `b` may
    /// not; `a <: Nothing` if `a`'s are unknown; `None` otherwise.
    pub fn compare_atoms(&mut self, a: TypeId, b: TypeId, known_singletons: bool) -> Option<bool> {
        let r2 = self.atoms(b)?;
        let Some(r1) = self.atoms(a) else { return Some(self.recur(a, NOTHING)) };
        if subset(&r1.hi, &r2.lo) || known_singletons && one(&r2.hi).is_some() && one(&r1.hi) == one(&r2.hi) {
            Some(true)
        } else if !subset(&r1.lo, &r2.hi) {
            Some(false)
        } else {
            None
        }
    }

    /// The atoms of `t`, `None` if unknown (dotty's `Type.atoms`).
    pub fn atoms(&mut self, t: TypeId) -> Option<Range> {
        self.atoms_at(t, 0)
    }

    /// The atoms of `t`, reached in `depth` steps that are not unions.
    fn atoms_at(&mut self, t: TypeId, depth: u32) -> Option<Range> {
        let mut r = Range::default();
        self.add_atoms(t, &mut r, depth).then_some(r)
    }

    /// Adds the atoms of `t` to `r`; false if they are unknown.
    fn add_atoms(&mut self, t: TypeId, r: &mut Range, depth: u32) -> bool {
        match *self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) => {
                r.lo.push(t);
                r.hi.push(t);
                true
            }
            Type::Or(x, y) => match (self.is_nothing(x, depth), self.is_nothing(y, depth)) {
                (true, _) => self.add_atoms(y, r, depth),
                (_, true) => self.add_atoms(x, r, depth),
                _ => self.add_atoms(x, r, depth) && self.add_atoms(y, r, depth),
            },
            _ if depth < MAX_DEPTH => match self.own_atoms(t, depth + 1) {
                Some(own) => {
                    r.lo.extend(own.lo);
                    r.hi.extend(own.hi);
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    /// The atoms of `t`, neither a constant nor a union.
    fn own_atoms(&mut self, t: TypeId, depth: u32) -> Option<Range> {
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Or(..) | Type::Constant(..) | Type::Str(_) => self.atoms_at(t, depth),
            Type::ByName(r) => self.atoms_at(r, depth),
            Type::Var(v) => self.atoms_at(self.vars.instance(v)?, depth),
            Type::And(x, y) => match (self.atoms_at(x, depth), self.atoms_at(y, depth)) {
                (Some(r1), Some(r2)) => {
                    let lo = r1.lo.iter().copied().filter(members(&r2.lo, r1.lo.len())).collect();
                    let hi = r1.hi.iter().copied().filter(members(&r2.hi, r1.hi.len())).collect();
                    Some(Range { lo, hi })
                }
                (Some(r), None) | (None, Some(r)) => Some(r.upper()),
                (None, None) => None,
            },
            Type::Refined(p, ..) | Type::Rec(_, p) | Type::Annotated(p, _) | Type::Match(p) => {
                Some(self.atoms_at(p, depth)?.upper())
            }
            _ if self.is_singleton(t) => self.singleton_atoms(t, depth),
            Type::TypeRef(..) | Type::Sym(..) => match self.named(t)?.meaning {
                Meaning::Abstract(_, hi) => Some(self.atoms_at(hi, depth)?.upper()),
                _ => None,
            },
            Type::Applied(tycon, args) => match self.named(tycon)?.meaning {
                Meaning::Abstract(_, hi) => {
                    let upper = self.apply(hi, &args);
                    Some(self.atoms_at(upper, depth)?.upper())
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// The atoms of singleton `t`: those of its underlying type if it has just one, else `t`
    /// normalized if it is stable.
    fn singleton_atoms(&mut self, t: TypeId, depth: u32) -> Option<Range> {
        let underlying = self.u.widen_once(t).and_then(|w| self.atoms_at(w, depth));
        match underlying {
            Some(r) if one(&r.hi).is_some() => Some(r),
            Some(r) => Some(r.upper()),
            None if self.is_stable(t) => {
                let atom = self.normalized(t, depth);
                Some(Range { lo: vec![atom], hi: vec![atom] })
            }
            None => None,
        }
    }

    /// Whether `t` has class symbol `Nothing` (dotty's `hasClassSymbol(defn.NothingClass)`).
    fn is_nothing(&mut self, t: TypeId, depth: u32) -> bool {
        if depth >= MAX_DEPTH {
            return false;
        }
        let (t, d) = (self.u.dealias(t), depth + 1);
        match self.u.types().get(t).clone() {
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(t).map(|n| n.meaning) {
                Some(Meaning::Class(c)) => c == self.u.builtins().nothing,
                Some(Meaning::Alias(r) | Meaning::Abstract(_, r)) => self.is_nothing(r, d),
                Some(Meaning::Term(_)) => self.u.widen_once(t).is_some_and(|w| self.is_nothing(w, d)),
                _ => false,
            },
            Type::Applied(tycon, args) => match self.named(tycon).map(|n| n.meaning) {
                Some(Meaning::Abstract(_, hi)) => {
                    let upper = self.apply(hi, &args);
                    self.is_nothing(upper, d)
                }
                _ => false,
            },
            Type::Refined(p, ..) | Type::Rec(_, p) | Type::Annotated(p, _) | Type::Match(p) => {
                self.is_nothing(p, d)
            }
            Type::Var(v) => self.vars.instance(v).is_some_and(|i| self.is_nothing(i, d)),
            Type::And(x, y) => self.is_nothing(x, d) || self.is_nothing(y, d),
            Type::Or(x, y) => self.is_nothing(x, d) && self.is_nothing(y, d),
            _ => false,
        }
    }
}

/// Whether every element of `xs` is one of `ys`.
pub fn subset(xs: &[TypeId], ys: &[TypeId]) -> bool {
    xs.iter().all(members(ys, xs.len()))
}

/// The element `xs` consists of, if it is not empty and has no other.
fn one(xs: &[TypeId]) -> Option<TypeId> {
    let (&x, rest) = xs.split_first()?;
    rest.iter().all(|&y| y == x).then_some(x)
}

/// A test for membership in `ys`, which `queries` tests will use; hashed if both are many.
pub fn members(ys: &[TypeId], queries: usize) -> impl Fn(&TypeId) -> bool + '_ {
    let set: Option<FxHashSet<TypeId>> =
        (ys.len() > 16 && queries > 16).then(|| ys.iter().copied().collect());
    move |x| set.as_ref().map_or_else(|| ys.contains(x), |s| s.contains(x))
}
