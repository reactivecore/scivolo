// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Atoms.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The singleton types a type consists of (dotty's `Atoms`), and comparisons decided by them
//! (`compareAtoms`).

use super::unions::List;
use super::{Cmp, MAX_DEPTH};
use crate::NOTHING;
use crate::types::{Type, TypeId};

/// The singletons a type must (`lo`) and may (`hi`) consist of, with repeats (dotty's
/// `Atoms.Range`).
#[derive(Clone, PartialEq, Debug)]
pub struct Range {
    pub lo: List,
    pub hi: List,
}

/// The atoms of a type.
#[derive(Clone, PartialEq, Debug)]
pub enum Atoms {
    Known(Range),
    /// dotty's `Atoms.Unknown`.
    Unknown,
    /// Atoms of a type this comparison does not compute; dotty's may be known.
    Unmodelled,
}

impl Atoms {
    pub fn single(t: TypeId) -> Atoms {
        Atoms::Known(Range { lo: List::Own(vec![t]), hi: List::Own(vec![t]) })
    }

    /// The atoms of a type known only to be below one with these atoms.
    pub fn upper(self) -> Atoms {
        match self {
            Atoms::Known(r) => Atoms::Known(Range { lo: List::Own(Vec::new()), hi: r.hi }),
            other => other,
        }
    }
}

impl Cmp<'_> {
    /// `a <: b` by atoms if `b`'s are known: true if `a` may only have atoms `b` must have, or if
    /// `known_singletons` and both are the same single atom; false if `a` must have one `b` may
    /// not; `a <: Nothing` if `a`'s are unknown; `None` otherwise.
    pub fn compare_atoms(&mut self, a: TypeId, b: TypeId, known_singletons: bool) -> Option<bool> {
        let Atoms::Known(r2) = self.atoms(b) else { return None };
        let r1 = match self.atoms(a) {
            Atoms::Known(r1) => r1,
            Atoms::Unknown => return Some(self.recur(a, NOTHING)),
            Atoms::Unmodelled => return None,
        };
        let us = self.u.unions();
        let single = known_singletons && us.one(&r2.hi).is_some() && us.one(&r1.hi) == us.one(&r2.hi);
        if single || us.subset(&r1.hi, &r2.lo) {
            Some(true)
        } else if !us.subset(&r1.lo, &r2.hi) {
            Some(false)
        } else {
            None
        }
    }

    /// The atoms of `t` (dotty's `Type.atoms`).
    pub fn atoms(&mut self, t: TypeId) -> Atoms {
        self.walk.clear();
        self.atoms_at(t, 0)
    }

    /// The atoms of `t`, reached in `depth` steps that are not unions; kept for the walk unless
    /// trivial or cached for the universe.
    pub fn atoms_at(&mut self, t: TypeId, depth: u32) -> Atoms {
        if let Some((atoms, provisional)) = self.walk.atoms(t, depth) {
            self.provisional |= provisional;
            return atoms;
        }
        if depth >= MAX_DEPTH || !self.step() {
            self.provisional = true;
            return Atoms::Unmodelled;
        }
        let outer = std::mem::replace(&mut self.provisional, false);
        let (atoms, elsewhere) = match *self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) => (Atoms::single(t), true),
            Type::Or(x, y, _) => (self.or_atoms(t, x, y, depth), true),
            _ => (self.own_atoms(t, depth + 1), false),
        };
        if self.provisional || !elsewhere {
            self.walk.set_atoms(t, depth, atoms.clone(), self.provisional);
        }
        self.provisional |= outer;
        atoms
    }

    /// The atoms of union `t = x | y`, cached unless provisional; entered not provisional.
    fn or_atoms(&mut self, t: TypeId, x: TypeId, y: TypeId, depth: u32) -> Atoms {
        if let Some(cached) = self.u.unions().atoms(t) {
            return cached;
        }
        let (nx, ny) = (self.nothing_at(x, depth), self.nothing_at(y, depth));
        let atoms = match (nx, ny) {
            (Some(true), _) => self.atoms_at(y, depth),
            (_, Some(true)) => self.atoms_at(x, depth),
            _ => {
                let (ax, ay) = (self.atoms_at(x, depth), self.atoms_at(y, depth));
                let (kept_x, kept_y) = (nx == Some(false), ny == Some(false));
                if kept_x && kept_y {
                    self.joined(ax, ay, !self.provisional)
                } else if (kept_x || ay == Atoms::Unknown) && (kept_y || ax == Atoms::Unknown) {
                    Atoms::Unknown
                } else {
                    Atoms::Unmodelled
                }
            }
        };
        if !self.provisional {
            self.u.unions().set_atoms(t, atoms.clone());
        }
        atoms
    }

    /// The atoms of a union of types with atoms `ax` and `ay`, its lists shared if `share`.
    pub fn joined(&mut self, ax: Atoms, ay: Atoms, share: bool) -> Atoms {
        let (rx, ry) = match (ax, ay) {
            (Atoms::Known(rx), Atoms::Known(ry)) => (rx, ry),
            (Atoms::Unknown, _) | (_, Atoms::Unknown) => return Atoms::Unknown,
            _ => return Atoms::Unmodelled,
        };
        let us = self.u.unions();
        let lo = us.joined(&rx.lo, &ry.lo, share);
        let hi = match rx.lo == rx.hi && ry.lo == ry.hi {
            true => lo.clone(),
            false => us.joined(&rx.hi, &ry.hi, share),
        };
        Atoms::Known(Range { lo, hi })
    }

    /// The atoms of `t`, neither a constant nor a union.
    fn own_atoms(&mut self, t: TypeId, depth: u32) -> Atoms {
        let dealiased = self.u.dealias(t);
        if dealiased != t {
            self.provisional = true;
        }
        let t = dealiased;
        match self.u.types().get(t).clone() {
            Type::Or(..) | Type::Constant(..) | Type::Str(_) => self.atoms_at(t, depth),
            Type::ByName(r) => self.atoms_at(r, depth),
            Type::Var(v) => {
                self.provisional = true;
                match self.vars.instance(v) {
                    Some(i) => self.atoms_at(i, depth),
                    None => self.atoms_at(self.vars.bounds(v).1, depth).upper(),
                }
            }
            Type::And(x, y) => self.and_atoms(x, y, depth),
            Type::Refined(p, ..) | Type::Rec(_, p) | Type::Annotated(p, _) => self.atoms_at(p, depth).upper(),
            Type::Match { bound, .. } => match self.atoms_at(bound, depth).upper() {
                Atoms::Known(r) => Atoms::Known(r),
                _ => Atoms::Unmodelled,
            },
            Type::Method { .. } | Type::Poly { .. } | Type::Bounds(..) | Type::Wildcard(_) => Atoms::Unknown,
            _ if self.is_singleton(t) => {
                self.provisional = true;
                self.singleton_atoms(t, depth)
            }
            Type::TypeRef(..) | Type::Sym(..) => {
                self.provisional = true;
                self.named_atoms(t, depth)
            }
            Type::Applied(tycon, args) => {
                self.provisional = true;
                self.applied_atoms(tycon, &args, depth)
            }
            _ => Atoms::Unmodelled,
        }
    }

    /// The atoms of `x & y` (dotty's `Atoms.&`), those of one side's upper bound if the other's
    /// are not computed.
    fn and_atoms(&mut self, x: TypeId, y: TypeId, depth: u32) -> Atoms {
        match (self.atoms_at(x, depth), self.atoms_at(y, depth)) {
            (Atoms::Known(r1), Atoms::Known(r2)) => {
                let us = self.u.unions();
                Atoms::Known(Range { lo: us.common(&r1.lo, &r2.lo), hi: us.common(&r1.hi, &r2.hi) })
            }
            (known @ Atoms::Known(_), _) | (_, known @ Atoms::Known(_)) => known.upper(),
            (Atoms::Unknown, Atoms::Unknown) => Atoms::Unknown,
            _ => Atoms::Unmodelled,
        }
    }
}
