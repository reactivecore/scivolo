// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Value classes unboxed in signatures (dotty's `jsig` of a derived value class).

use super::{Boxing, Sig};
use crate::codegen::descriptors::primitive_box;
use tasty::{Erased, SymbolId, TypeId};

impl Sig<'_> {
    /// Value class `c` of underlying value `value` referred to by `t`, as the value's type seen
    /// from `t`: a primitive standing for a type parameter as its box, an array as its erasure.
    pub fn value_class(&mut self, t: TypeId, c: SymbolId, value: SymbolId) {
        let underlying = self.u.info(value);
        let seen = self.u.as_seen_from(underlying, t, c);

        let declared_primitive = self.is_primitive(underlying);
        if !declared_primitive && self.is_primitive(seen) {
            return match self.u.erase(seen) {
                Erased::Unit => self.out.push_str("Lscala/runtime/BoxedUnit;"),
                e => self.class_sig(primitive_box(&e).unwrap_or_default(), &[]),
            };
        }
        let array = self.u.class_symbol(underlying).is_some_and(|a| self.u.full_name(a) == "scala.Array");
        match array {
            true => {
                let e = self.u.erase(underlying);
                self.erased_sig(&e, Boxing::Unbox);
            }
            false => self.jsig(seen, false, Boxing::Unbox),
        }
    }

    /// Whether `t` derives from `AnyVal`.
    pub fn is_value(&mut self, t: TypeId) -> bool {
        let any_val = self.u.defn().any_val;
        self.u.class_symbol(t).is_some_and(|c| any_val.is_some_and(|v| self.u.derives_from(c, v)))
    }

    /// Whether `t` is a primitive class (dotty's `isPrimitiveValueType`).
    fn is_primitive(&mut self, t: TypeId) -> bool {
        self.class_ref(t).is_some() && matches!(self.u.erase(t), Erased::Prim(_) | Erased::Unit)
    }
}
