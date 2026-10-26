// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Mangle.scala
//! The mangled signatures of constructors.

use super::mangle::mangle;
use crate::{SymbolId, Universe};

impl Universe {
    /// The mangled signature of constructor `ctor` of a static class, e.g. `RiE`.
    pub fn constructor_signature(&mut self, ctor: SymbolId) -> String {
        let (params, _) = self.erased_method(ctor);
        format!("R{}E", params.iter().map(mangle).collect::<String>())
    }
}
