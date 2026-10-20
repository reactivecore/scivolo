// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! What a symbol is (dotty's `SymDenotation` queries `isClass`, `is(Module)`, `isType`).

use super::{Kind, SymbolId, Universe};
use crate::tags::{OBJECT, TRAIT};

/// Whether `name` is the name of a package object's class: `package$`, or a source file's
/// `F$package$`.
pub fn is_package_object_name(name: &str) -> bool {
    name == "package$" || name.ends_with("$package$")
}

impl Universe {
    pub fn is_class(&self, s: SymbolId) -> bool {
        self.symbol(s).kind == Kind::Class
    }

    pub fn is_module_class(&self, s: SymbolId) -> bool {
        self.is_class(s) && self.symbol(s).flags.has(OBJECT)
    }

    /// A module value, not its class.
    pub fn is_module(&self, s: SymbolId) -> bool {
        self.symbol(s).kind.is_term() && self.symbol(s).flags.has(OBJECT)
    }

    pub fn is_trait(&self, s: SymbolId) -> bool {
        self.is_class(s) && self.symbol(s).flags.has(TRAIT)
    }

    pub fn is_type_symbol(&self, s: SymbolId) -> bool {
        self.symbol(s).kind.is_type()
    }

    pub fn is_constructor(&self, s: SymbolId) -> bool {
        self.symbol(s).kind == Kind::Def && self.symbol(s).name == "<init>"
    }

    /// The class of a package object.
    pub fn is_package_object(&self, s: SymbolId) -> bool {
        self.is_module_class(s) && is_package_object_name(&self.symbol(s).name)
    }

    /// Whether `s` is reached through static paths only: its owner is a package or an object
    /// that is itself static.
    pub fn is_static_symbol(&self, s: SymbolId) -> bool {
        match self.symbol(s).owner {
            None => true,
            Some(o) => {
                let owner = self.symbol(o);
                owner.kind == Kind::Package
                    || (owner.kind == Kind::Class && owner.flags.has(OBJECT) && self.is_static_symbol(o))
            }
        }
    }
}
