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
        if let Type::Flexible(hi) = *self.u.ty(p) {
            let any = self.from_java_object(method, hi);
            return if any == hi { p } else { self.u.flexible(any) };
        }
        let object = self.u.defn().object;
        let dealiased = self.u.dealias(p);
        let is_object = matches!(self.u.ty(dealiased), Type::Sym(..) | Type::TypeRef(..))
            && self.u.class_symbol(dealiased) == Some(object);
        let java = method.is_some_and(|m| !self.u.is_entered_object_member(m) && self.is_java_member(m));
        if is_object && java { ANY } else { p }
    }

    /// `FromJavaObject` for element type `elem` of a repeated parameter of `method` that
    /// `from_java_object` makes `Any`, `elem` otherwise (dotty's element type of the arguments).
    pub fn java_object_elem(&mut self, method: Option<SymbolId>, elem: TypeId) -> TypeId {
        if self.from_java_object(method, elem) != ANY {
            return elem;
        }
        let from_java_object = self.u.defn().from_java_object;
        self.u.intern_type(Type::Sym(None, from_java_object))
    }
}
