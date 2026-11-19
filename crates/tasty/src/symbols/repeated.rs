// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//! Repeated parameter types, `T*`, and the types `ElimRepeated` gives them.

use super::Universe;
use super::defn::IMMUTABLE_SEQ;
use crate::types::{Type, TypeId};

const REPEATED_ANNOT: &str = "scala.annotation.internal.Repeated";
const REPEATED_CLASS: &str = "scala.<repeated>";

impl Universe {
    /// The element type `T` of repeated parameter type `t`: `<repeated>[T]` or `Seq[T] @Repeated`
    /// as TASTy gives it.
    pub fn repeated_elem(&mut self, t: TypeId) -> Option<TypeId> {
        let ty = *self.ty(t);
        let applied = match ty {
            Type::Annotated(seq, annot) if self.is_class_named(annot, REPEATED_ANNOT) => seq,
            Type::Applied(c, _) if self.is_class_named(c, REPEATED_CLASS) => t,
            _ => return None,
        };
        match self.applied(applied)? {
            (_, &[elem]) => Some(elem),
            _ => None,
        }
    }

    /// Translates repeated parameter type `t` as `ElimRepeated` does: `<repeated>[T]` to `Seq[T]`,
    /// `Seq[T] @Repeated` or Java's `Array[T] @Repeated` to its annotated type; another `t` stays
    /// (dotty's `translateFromRepeated`).
    pub fn from_repeated(&mut self, t: TypeId) -> TypeId {
        let ty = *self.ty(t);
        match ty {
            Type::Annotated(inner, annot) if self.is_class_named(annot, REPEATED_ANNOT) => inner,
            Type::Applied(c, _) if self.is_class_named(c, REPEATED_CLASS) => {
                let Some(elem) = self.repeated_elem(t) else { return t };
                let seq = self.class_ref(IMMUTABLE_SEQ);
                self.intern_applied(seq, &[elem])
            }
            _ => t,
        }
    }
}
