// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Greatest lower bounds of ground types (dotty's `glb`).

use super::hk::Connective;
use super::{Cmp, NoVars, TypeVars};
use crate::types::{Type, TypeId};
use crate::{NOTHING, Universe};

impl Universe {
    /// The greatest lower bound of `a` and `b`: one of them if the other contains it, else
    /// their intersection, arguments of a common constructor merged.
    pub fn glb(&mut self, a: TypeId, b: TypeId) -> TypeId {
        Cmp::new(self, &mut NoVars).glb(a, b)
    }

    /// `glb` with the type variables of `vars`, compared frozen.
    pub fn glb_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> TypeId {
        let mut c = Cmp::new(self, vars);
        c.frozen = true;
        c.glb(a, b)
    }
}

impl Cmp<'_> {
    /// Computes the greatest lower bound of `a` and `b` within a running comparison (dotty's `glb`):
    /// - `a` if both are equal, `b` is `Any` or `a` is `Nothing`, and the other way round;
    /// - the parts of an intersection that are supertypes of the other type dropped, the other
    ///   type if no part remains;
    /// - distributed over unions: `(A | B) & C` is `(A & C) | (B & C)`;
    /// - `Nothing` for two different literal types, else the intersection `a & b`.
    pub fn glb(&mut self, a: TypeId, b: TypeId) -> TypeId {
        if a == b || self.is_top(b, a) || self.is_bottom(a) {
            return a;
        }
        if self.is_top(a, b) || self.is_bottom(b) {
            return b;
        }
        match self.drop_if_super(a, b) {
            Some(rest) if rest != a => return self.glb(rest, b),
            None => return b,
            _ => {}
        }
        match self.drop_if_super(b, a) {
            Some(rest) if rest != b => return self.glb(a, rest),
            None => return a,
            _ => {}
        }
        match (*self.u.ty(a), *self.u.ty(b)) {
            (_, Type::Or(l, r, soft)) => {
                let (al, ar) = (self.glb(a, l), self.glb(a, r));
                self.lub_as(al, ar, soft)
            }
            (Type::Or(l, r, soft), _) => {
                let (lb, rb) = (self.glb(l, b), self.glb(r, b));
                self.lub_as(lb, rb, soft)
            }
            (Type::Constant(..) | Type::Str(_), Type::Constant(..) | Type::Str(_)) => NOTHING,
            _ => self.and_type(a, b),
        }
    }

    /// `t` without its `&`-parts that are supertypes of `sub`; `None` if none remains.
    fn drop_if_super(&mut self, t: TypeId, sub: TypeId) -> Option<TypeId> {
        match *self.u.ty(t) {
            Type::And(x, y) => match (self.drop_if_super(x, sub), self.drop_if_super(y, sub)) {
                (Some(x2), Some(y2)) if (x2, y2) == (x, y) => Some(t),
                (Some(x2), Some(y2)) => Some(self.u.intern_type(Type::And(x2, y2))),
                (Some(r), None) | (None, Some(r)) => Some(r),
                (None, None) => None,
            },
            _ => (!self.recur_frozen(sub, t)).then_some(t),
        }
    }

    /// `a & b`, merged into one application of a common constructor where all arguments merge, and
    /// with a wildcard among them where that gives a type below both; of an annotated type, its
    /// parent's with the other type, `(A & B)` of `A @x & B` and `B @x & A` (dotty's
    /// `distributeAnd` both ways, the applications merged only where `distributes_and`).
    pub fn and_type(&mut self, a: TypeId, b: TypeId) -> TypeId {
        if self.u.distributes_and
            && let (Some((c1, args1)), Some((c2, args2))) = (self.u.applied(a), self.u.applied(b))
            && let (args1, args2) = (args1.to_vec(), args2.to_vec())
            && args1.len() == args2.len()
            && self.same_constructor(c1, c2)
        {
            let variances = self.variances(c1, args1.len());
            let args: Option<Vec<TypeId>> =
                (0..args1.len()).map(|i| self.glb_arg(args1[i], args2[i], variances[i])).collect();
            if let Some(args) = args {
                let wildcards = args1.iter().chain(&args2).any(|&x| self.arg_bounds(x).is_some());
                let merged = self.u.intern_applied(c1, &args);
                if !wildcards || self.recur_frozen(merged, a) && self.recur_frozen(merged, b) {
                    return merged;
                }
            }
        }
        match (*self.u.ty(a), *self.u.ty(b)) {
            (Type::Annotated(x, _), _) => self.glb(x, b),
            (_, Type::Annotated(y, _)) => self.glb(y, a),
            (Type::ByName(x), Type::ByName(y)) => {
                let both = self.glb(x, y);
                self.u.intern_type(Type::ByName(both))
            }
            _ => self.lift_if_constructors(a, b, Connective::And),
        }
    }

    /// The argument of an intersection of two applications at a parameter of `variance`
    /// (dotty's `glbArgs`): the single type between them (dotty's `singletonInterval`: the first
    /// of two equivalent ones, `"a"` of `"a"` and `Singleton` or `String & Singleton` in either
    /// order), the glb of covariant and lub of contravariant ones, the merged bounds of wildcards;
    /// `None` for different invariant arguments.
    fn glb_arg(&mut self, x: TypeId, y: TypeId, variance: i8) -> Option<TypeId> {
        if x == y {
            return Some(x);
        }
        let x_below = self.recur_frozen(x, y);
        let y_singleton = self.is_singleton_ref(y);
        let y_below = (x_below || variance != 0 || y_singleton) && self.recur_frozen(y, x);
        if x_below && y_below || x_below && self.is_singleton_below(x, y) {
            return Some(x);
        }
        if y_below && y_singleton && self.derives_from_singleton(x) {
            return Some(y);
        }

        let (bx, by) = (self.arg_bounds(x), self.arg_bounds(y));
        if variance == 1 && bx.is_none() && by.is_none() && !self.is_connective(x) && !self.is_connective(y) {
            match (x_below, y_below) {
                (true, _) => return Some(x),
                (_, true) => return Some(y),
                _ => {}
            }
        }
        let (lo_x, hi_x) = bx.unwrap_or((x, x));
        let (lo_y, hi_y) = by.unwrap_or((y, y));
        match variance {
            1 => Some(self.glb(hi_x, hi_y)),
            -1 => Some(self.lub(lo_x, lo_y)),
            _ if bx.is_some() || by.is_some() => {
                let (lo, hi) = (self.lub(lo_x, lo_y), self.glb(hi_x, hi_y));
                Some(self.u.intern_type(Type::Bounds(lo, hi)))
            }
            _ => None,
        }
    }

    /// Whether `lo`, known to be below `hi`, is a singleton and `hi` derives from `Singleton`.
    fn is_singleton_below(&mut self, lo: TypeId, hi: TypeId) -> bool {
        self.is_singleton_ref(lo) && self.derives_from_singleton(hi)
    }

    fn is_singleton_ref(&mut self, t: TypeId) -> bool {
        let dealiased = self.u.dealias(t);
        self.is_singleton(dealiased)
    }

    fn derives_from_singleton(&mut self, t: TypeId) -> bool {
        let singleton = self.u.defn().singleton;
        self.u.base_type(t, singleton).is_some()
    }

    fn is_connective(&self, t: TypeId) -> bool {
        matches!(self.u.ty(t), Type::And(..) | Type::Or(..))
    }

    /// Whether constructors `c1` and `c2` refer to the same class or type.
    fn same_constructor(&mut self, c1: TypeId, c2: TypeId) -> bool {
        match (self.named(c1).and_then(|n| n.sym), self.named(c2).and_then(|n| n.sym)) {
            (Some(s1), Some(s2)) => s1 == s2 && self.recur_frozen(c1, c2) && self.recur_frozen(c2, c1),
            _ => false,
        }
    }
}
