// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Base types that several parents of a class give a base class, intersected (dotty's
//! `baseTypeOf` folding the parents with `&`, `glbArgs`).

use super::{SymbolId, Universe};
use crate::tags::{CONTRAVARIANT, COVARIANT};
use crate::types::{Type, TypeId};

impl Universe {
    /// The base types `x` and `y` of the parts of intersection (`and`) or union `t`, or of two
    /// parents of class type `t`, merged by `glb` or `lub`; while that merge is in progress, `x`
    /// of an intersection, else `None`; the cut is counted, and for a class of the sources being
    /// compiled `base` is kept for `take_cyclic_base_class`.
    pub fn merged_bases(
        &mut self,
        t: TypeId,
        base: SymbolId,
        (x, y): (TypeId, TypeId),
        and: bool,
    ) -> Option<TypeId> {
        if x == y {
            return Some(x);
        }
        if !self.merging_bases.insert((t, base)) {
            self.base_type_cuts += 1;
            let connective = matches!(self.ty(t), Type::And(..) | Type::Or(..));
            if !connective && self.class_symbol(t).is_some_and(|c| self.symbol(c).tree.is_none()) {
                self.cyclic_base_class.get_or_insert(base);
            }
            return and.then_some(x);
        }
        let merged = if and { self.glb(x, y) } else { self.lub(x, y) };
        self.merging_bases.remove(&(t, base));
        Some(merged)
    }

    /// Takes the base class whose type a class's parents needed to compute that very type, if
    /// any since the last call (dotty's `CyclicReference` of `baseTypeOf`).
    pub fn take_cyclic_base_class(&mut self) -> Option<SymbolId> {
        self.cyclic_base_class.take()
    }

    /// Base types `a` and `b` of class `base` intersected, argument by argument as their
    /// parameters' variances allow; `None` if an invariant argument differs.
    pub fn joined_base_type(&mut self, base: SymbolId, a: TypeId, b: TypeId) -> Option<TypeId> {
        let (Some((tycon, xs)), Some((_, ys))) = (self.applied(a), self.applied(b)) else {
            return (a == b).then_some(a);
        };
        let (xs, ys) = (xs.to_vec(), ys.to_vec());
        let params = self.type_params(base);
        let mut args = Vec::with_capacity(xs.len());
        for ((x, y), p) in xs.into_iter().zip(ys).zip(params) {
            let flags = self.symbol(p).flags;
            let (co, contra) = (flags.has(COVARIANT), flags.has(CONTRAVARIANT));
            let below = x == y || self.is_subtype(x, y);
            let above = x == y || self.is_subtype(y, x);
            args.push(match (below, above) {
                (true, true) => x,
                _ if !co && !contra => return None,
                (true, false) | (false, true) if co == below => x,
                (true, false) | (false, true) => y,
                _ if co => self.intern_type(Type::And(x, y)),
                _ => self.intern_type(Type::Or(x, y, true)),
            });
        }
        Some(self.intern_applied(tycon, &args))
    }

    /// Determines the argument a union of base types `base` gives the parameter at `index` of
    /// `variance` (dotty's `argForParam` of an `OrType`): the lub of the parts' covariant
    /// arguments, the glb of contravariant ones; `None` for different invariant ones or wildcards.
    pub fn union_base_arg(
        &mut self,
        base: TypeId,
        index: usize,
        variance: i32,
        arg: fn(&mut Self, TypeId, usize) -> Option<TypeId>,
    ) -> Option<TypeId> {
        let Type::Or(a, b, _) = *self.ty(base) else { return arg(self, base, index) };
        let (x, y) =
            (self.union_base_arg(a, index, variance, arg)?, self.union_base_arg(b, index, variance, arg)?);
        if x == y {
            return Some(x);
        }
        if self.bounds(x).is_some() || self.bounds(y).is_some() {
            return None;
        }
        match variance {
            1 => Some(self.lub(x, y)),
            -1 => Some(self.glb(x, y)),
            _ => None,
        }
    }
}
