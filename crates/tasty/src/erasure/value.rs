// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Semi-erasure (dotty's `valueErasure`): a value class's value as `Erased::Value`, its class
//! with the erasure of its underlying value.

use super::Erased;
use crate::types::{Type, TypeId};
use crate::{SymbolId, Universe};

impl Erased {
    /// The erasure Scala Native's backend sees: a value class's value as its underlying value.
    pub fn full(self) -> Erased {
        match self {
            Erased::Value(_, underlying) => *underlying,
            e => e,
        }
    }
}

impl Universe {
    /// The semi-erasure of `t`: `Value` for a value class other than `this` or a by-name type,
    /// else its erasure.
    pub fn erase_value(&mut self, t: TypeId) -> Erased {
        match *self.types().get(t) {
            Type::This(class) => return self.erase_boxed(class),
            Type::ByName(_) => return self.erase(t),
            _ => {}
        }
        let widened = self.widen(t);
        match self.value_class(widened) {
            Some(c) => Erased::Value(self.binary_name(c), Box::new(self.erase(widened))),
            None => self.erase(t),
        }
    }

    /// The value class `t` refers to, if any; of an intersection, that of either side (dotty's
    /// `erasedGlb` ranks value classes first).
    pub fn value_class(&mut self, t: TypeId) -> Option<SymbolId> {
        let dealiased = self.dealias(t);
        if let Type::And(a, b) = *self.types().get(dealiased) {
            return self.value_class(a).or_else(|| self.value_class(b));
        }
        self.class_symbol(t).filter(|&c| self.underlying(c).is_some())
    }
}
