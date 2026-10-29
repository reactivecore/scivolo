// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Erased parameters: those whose type in their method's type is annotated `@ErasedParam`
//! (dotty's `paramErasureStatuses`).

use crate::Universe;
use crate::types::{Type, TypeId};

impl Universe {
    /// Whether parameter type `t` of a method type is annotated `@ErasedParam`.
    pub fn is_erased_param(&mut self, t: TypeId) -> bool {
        let t = match *self.types().get(t) {
            Type::ByName(r) => r,
            _ => t,
        };
        let mut t = t;
        while let Type::Annotated(inner, annot) = *self.types().get(t) {
            let class = self.class_symbol(annot);
            if class.is_some_and(|c| self.full_name(c) == "scala.annotation.internal.ErasedParam") {
                return true;
            }
            t = inner;
        }
        false
    }

    /// Whether method type `t` has an erased parameter in its first clause.
    pub fn has_erased_params(&mut self, t: TypeId) -> bool {
        let params = self.method(t).map(|m| m.param_types()).unwrap_or_default();
        params.into_iter().any(|p| self.is_erased_param(p))
    }
}
