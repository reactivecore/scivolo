// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Flexible types `T?` and `T | Null` (dotty's `FlexibleType`, `OrNull`).

use super::{Type, TypeId};
use crate::Universe;

impl Universe {
    /// The hard union `t | Null` (dotty's `OrNull`).
    pub fn or_null(&mut self, t: TypeId) -> TypeId {
        let null = self.class_ref("scala.Null");
        self.intern_type(Type::Or(t, null, false))
    }

    /// `t?`, `t` itself if it is flexible or a union with `Null`, bounds made flexible
    /// (dotty's `FlexibleType.make`).
    pub fn flexible(&mut self, t: TypeId) -> TypeId {
        let ty = *self.ty(t);
        match ty {
            Type::Flexible(_) => t,
            Type::Or(_, b, _) if self.is_null_ref(b) => t,
            Type::Bounds(lo, hi) => {
                let (lo, hi) = (self.flexible(lo), self.flexible(hi));
                self.intern_type(Type::Bounds(lo, hi))
            }
            Type::Wildcard(Some(b)) => {
                let b = self.flexible(b);
                self.intern_type(Type::Wildcard(Some(b)))
            }
            Type::Wildcard(None) => t,
            _ => self.intern_type(Type::Flexible(t)),
        }
    }

    /// Whether `t` refers to class `Null`.
    pub fn is_null_ref(&mut self, t: TypeId) -> bool {
        let null = self.defn().null;
        matches!(self.ty(t), Type::Sym(..) | Type::TypeRef(..)) && self.referenced(t) == Some(null)
    }
}
