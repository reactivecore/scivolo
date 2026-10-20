// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Repeated parameters that may be by-name, `=> T*` (dotty's `isRepeatedParam` through an
//! `ExprType`).

use super::Typer;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The element type of repeated parameter type `p`, by-name or not.
    pub fn repeated_param_elem(&mut self, p: TypeId) -> Option<TypeId> {
        match *self.u.types().get(p) {
            Type::ByName(r) => self.repeated_elem(r),
            _ => self.repeated_elem(p),
        }
    }

    /// Parameter type `p` with a repeated type, by-name or not, typed `<repeated>[T]`.
    pub fn with_repeated_param_class(&mut self, p: TypeId) -> TypeId {
        let Some(elem) = self.repeated_param_elem(p) else { return p };
        let repeated = self.repeated_type(elem);
        match *self.u.types().get(p) {
            Type::ByName(_) => self.u.intern_type(Type::ByName(repeated)),
            _ => repeated,
        }
    }
}
