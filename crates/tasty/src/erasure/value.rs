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
    /// The semi-erasure of `t`: `Value` for a value class other than `this`, else its erasure.
    pub fn erase_value(&mut self, t: TypeId) -> Erased {
        if let Type::This(class) = *self.types().get(t) {
            return self.erase_boxed(class);
        }
        let widened = self.widen(t);
        match self.value_class(widened) {
            Some(c) => Erased::Value(self.binary_name(c), Box::new(self.erase(widened))),
            None => self.erase(t),
        }
    }

    /// The value class `t` refers to, if any.
    pub fn value_class(&mut self, t: TypeId) -> Option<SymbolId> {
        self.class_symbol(t).filter(|&c| self.underlying(c).is_some())
    }
}
