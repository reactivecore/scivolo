// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! `Object` parameters of Java members, which take values of any type (dotty's
//! `FromJavaObject`).

use super::Typer;
use tasty::{ANY, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `Any` for parameter type `p` of `method` if `p` is `java.lang.Object` and `method` a member
    /// of a Java class, `Any?` if it is `Object?`; `p` otherwise.
    pub fn from_java_object(&mut self, method: Option<SymbolId>, p: TypeId) -> TypeId {
        if let Type::Flexible(hi) = *self.u.types().get(p) {
            let any = self.from_java_object(method, hi);
            return if any == hi { p } else { self.u.flexible(any) };
        }
        let object = self.u.builtins().object;
        let dealiased = self.u.dealias(p);
        let is_object = matches!(self.u.types().get(dealiased), Type::Sym(..) | Type::TypeRef(..))
            && self.u.class_symbol(dealiased) == Some(object);
        let java = method.is_some_and(|m| !self.u.is_entered_object_member(m) && self.is_java_member(m));
        if is_object && java { ANY } else { p }
    }
}
