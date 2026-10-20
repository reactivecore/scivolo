// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Polymorphic function types `PolyFunction { def apply[T](x: T): R }` and their erasure to the
//! function class of their arity.

use super::Erased;
use super::special::special;
use crate::Universe;
use crate::types::{Type, TypeId};

impl Universe {
    /// The info of `apply` if `t` is `PolyFunction { def apply: info }` or a refinement of a
    /// subclass of `PolyFunction` (dotty's `PolyFunctionOf.unapply`).
    pub fn poly_function_apply(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Refined(parent, name, info) = self.types().get(t).clone() else { return None };
        if name != "apply" || !matches!(self.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
            return None;
        }
        let poly_function = self.class_ref("scala.PolyFunction");
        let poly_function = self.class_symbol(poly_function)?;
        let class = self.class_symbol(parent)?;
        self.derives_from(class, poly_function).then_some(info)
    }

    /// The info of the `apply` of the polymorphic function type `t` is or is bounded by: through
    /// singletons, aliases, upper bounds and either side of an intersection.
    pub fn poly_function_apply_of(&mut self, t: TypeId) -> Option<TypeId> {
        let mut t = t;
        for _ in 0..16 {
            let widened = self.widen(t);
            t = self.dealias(widened);
            match self.types().get(t).clone() {
                Type::Refined(..) => return self.poly_function_apply(t),
                Type::And(a, b) => {
                    return self.poly_function_apply_of(a).or_else(|| self.poly_function_apply_of(b));
                }
                Type::Sym(_, s) if self.is_type_symbol(s) => {
                    let info = self.info(s);
                    let Type::Bounds(_, hi) = *self.types().get(info) else { return None };
                    t = hi;
                }
                _ => return None,
            }
        }
        None
    }

    /// The function class of the arity of `apply` info `info` (dotty's
    /// `eraseRefinedFunctionApply`).
    pub fn erase_refined_function_apply(&mut self, info: TypeId) -> Erased {
        let mut t = info;
        let arity = loop {
            match self.types().get(t) {
                Type::Poly { result, .. } => t = *result,
                Type::Method { params, .. } => break params.len(),
                _ => break 0,
            }
        };
        let full = format!("scala.Function{arity}");
        special(&full).unwrap_or(Erased::Class(full))
    }
}
