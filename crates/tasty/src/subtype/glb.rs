// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Greatest lower bounds of ground types (dotty's `glb`).

use super::hk::Connective;
use super::{Cmp, NoVars};
use crate::types::{Type, TypeId};
use crate::{NOTHING, Universe};

impl Universe {
    /// The greatest lower bound of `a` and `b`: one of them if the other contains it, else
    /// their intersection, arguments of a common constructor merged.
    pub fn glb(&mut self, a: TypeId, b: TypeId) -> TypeId {
        Cmp::new(self, &mut NoVars).glb(a, b)
    }
}

impl Cmp<'_> {
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
        match (self.u.types().get(a), self.u.types().get(b)) {
            (_, &Type::Or(l, r, soft)) => {
                let (al, ar) = (self.glb(a, l), self.glb(a, r));
                self.lub_as(al, ar, soft)
            }
            (&Type::Or(l, r, soft), _) => {
                let (lb, rb) = (self.glb(l, b), self.glb(r, b));
                self.lub_as(lb, rb, soft)
            }
            (Type::Constant(..) | Type::Str(_), Type::Constant(..) | Type::Str(_)) => NOTHING,
            _ => self.and_type(a, b),
        }
    }

    /// `t` without its `&`-parts that are supertypes of `sub`; `None` if none remains.
    fn drop_if_super(&mut self, t: TypeId, sub: TypeId) -> Option<TypeId> {
        match *self.u.types().get(t) {
            Type::And(x, y) => match (self.drop_if_super(x, sub), self.drop_if_super(y, sub)) {
                (Some(x2), Some(y2)) if (x2, y2) == (x, y) => Some(t),
                (Some(x2), Some(y2)) => Some(self.u.intern_type(Type::And(x2, y2))),
                (Some(r), None) | (None, Some(r)) => Some(r),
                (None, None) => None,
            },
            _ => (!self.recur_frozen(sub, t)).then_some(t),
        }
    }

    /// `a & b`, merged into one application of a common constructor where the merged arguments
    /// give a type below both.
    fn and_type(&mut self, a: TypeId, b: TypeId) -> TypeId {
        if let (Type::Applied(c1, args1), Type::Applied(c2, args2)) =
            (self.u.types().get(a).clone(), self.u.types().get(b).clone())
            && args1.len() == args2.len()
            && self.same_constructor(c1, c2)
        {
            let variances = self.variances(c1, args1.len());
            let args: Option<Vec<TypeId>> =
                (0..args1.len()).map(|i| self.glb_arg(args1[i], args2[i], variances[i])).collect();
            if let Some(args) = args {
                let merged = self.u.intern_type(Type::Applied(c1, args));
                if self.recur_frozen(merged, a) && self.recur_frozen(merged, b) {
                    return merged;
                }
            }
        }
        match (self.u.types().get(a), self.u.types().get(b)) {
            (&Type::Annotated(x, _), _) => self.glb(x, b),
            (_, &Type::Annotated(y, _)) => self.glb(a, y),
            (&Type::ByName(x), &Type::ByName(y)) => {
                let both = self.glb(x, y);
                self.u.intern_type(Type::ByName(both))
            }
            _ => self.lift_if_constructors(a, b, Connective::And),
        }
    }

    /// The argument of an intersection of two applications at a parameter of `variance`
    /// (dotty's `glbArgs`): the common one, the glb of covariant and lub of contravariant
    /// ones, the merged bounds of wildcards; `None` for different invariant arguments.
    fn glb_arg(&mut self, x: TypeId, y: TypeId, variance: i8) -> Option<TypeId> {
        if x == y {
            return Some(x);
        }
        let (bx, by) = (self.arg_bounds(x), self.arg_bounds(y));
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

    /// Whether constructors `c1` and `c2` refer to the same class or type.
    fn same_constructor(&mut self, c1: TypeId, c2: TypeId) -> bool {
        match (self.named(c1).and_then(|n| n.sym), self.named(c2).and_then(|n| n.sym)) {
            (Some(s1), Some(s2)) => s1 == s2 && self.recur_frozen(c1, c2) && self.recur_frozen(c2, c1),
            _ => false,
        }
    }
}
