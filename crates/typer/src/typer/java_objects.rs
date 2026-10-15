//! `Object` parameters of Java members, which take values of any type (dotty's
//! `FromJavaObject`).

use super::Typer;
use tasty::{ANY, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `Any` for parameter type `p` of `method` if `p` is `java.lang.Object` and `method` a member
    /// of a Java class; `p` otherwise.
    pub fn from_java_object(&mut self, method: Option<SymbolId>, p: TypeId) -> TypeId {
        let object = self.u.builtins().object;
        let dealiased = self.u.dealias(p);
        let is_object = matches!(self.u.types().get(dealiased), Type::Sym(..) | Type::TypeRef(..))
            && self.u.class_symbol(dealiased) == Some(object);
        if is_object && method.is_some_and(|m| self.is_java_member(m)) { ANY } else { p }
    }
}
