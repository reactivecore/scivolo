// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! The erasure of references to symbols.

use super::special::special;
use super::{Erased, object};
use crate::tags::OBJECT as MODULE;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

impl Universe {
    /// The erasure of member `s` of `prefix`: of the info a refinement of the prefix's type gives
    /// abstract type `s`, of the alias by which the prefix's class overrides it, else of `s`.
    pub fn erase_member(&mut self, prefix: TypeId, s: SymbolId) -> Erased {
        if self.symbol(s).kind == Kind::Type && self.alias_of(s).is_none() {
            if let Some(refined) = self.refinement_info(prefix, s) {
                return self.erase(refined);
            }
            if let Some(a) = self.overriding_alias(prefix, s) {
                return self.erase(a);
            }
        }
        self.erase_symbol(s)
    }

    /// The erasure of a reference to `s`: a class, an abstract type's bound, an alias, an
    /// object's class, a value's type.
    pub fn erase_symbol(&mut self, s: SymbolId) -> Erased {
        match self.symbol(s).kind {
            Kind::Class => {
                let full = self.full_name(s);
                if let Some(e) = special(&full) {
                    return e;
                }
                match self.underlying(s) {
                    Some(value) => {
                        let info = self.info(value);
                        self.erase(info)
                    }
                    None => Erased::Class(self.binary_name(s)),
                }
            }
            Kind::Val if self.symbol(s).flags.has(MODULE) => match self.module_class(s) {
                Some(c) => Erased::Class(self.binary_name(c)),
                None => object(),
            },
            Kind::Type | Kind::TypeParam | Kind::Val | Kind::Param => {
                let info = self.opaque_alias(s).unwrap_or_else(|| self.info(s));
                self.erase(info)
            }
            Kind::Def => {
                let info = self.info(s);
                match *self.types().get(info) {
                    Type::ByName(result) => self.erase(result),
                    Type::Method { .. } | Type::Poly { .. } => object(),
                    _ => self.erase(info),
                }
            }
            _ => object(),
        }
    }
}
