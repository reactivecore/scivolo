// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Applications of applied type constructors, `F[A][B]` (dotty's `compareAppliedType1` and
//! `compareAppliedType2` on an `AppliedType` constructor).

use super::Cmp;
use super::refs::Meaning;
use crate::types::{Type, TypeId};

impl Cmp<'_> {
    /// `a <: tycon[args]` for applied `tycon`: `fourthTry`, else against `tycon`'s lower bound
    /// applied to `args`.
    pub fn compare_curried2(&mut self, a: TypeId, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        match self.bound(tycon, false) {
            Some(lo) => {
                let lower = self.apply(lo, args);
                self.either(|s| s.fourth_try(a, b), |s| s.recur(a, lower))
            }
            None => self.fourth_try(a, b),
        }
    }

    /// `tycon[args] <: b` for applied `tycon`: through its super type.
    pub fn compare_curried1(&mut self, a: TypeId, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        let Some(hi) = self.bound(tycon, true) else { return false };
        let upper = self.apply(hi, args);
        upper != a && self.recur(upper, b)
    }

    /// The upper bound of type constructor `t` (dotty's `superType`), or its lower one
    /// (`lowerBound`); `None` for classes and constructors without bounds.
    fn bound(&mut self, t: TypeId, upper: bool) -> Option<TypeId> {
        match self.u.ty(t).clone() {
            Type::Applied(tycon, args) => {
                let b = self.bound(tycon, upper)?;
                let args = self.u.types_of(args).to_vec();
                Some(self.apply(b, &args))
            }
            Type::TypeRef(..) | Type::Sym(..) => match self.named(t)?.meaning {
                Meaning::Alias(alias) => Some(alias),
                Meaning::Abstract(lo, hi) => Some(if upper { hi } else { lo }),
                _ => None,
            },
            _ => None,
        }
    }
}
