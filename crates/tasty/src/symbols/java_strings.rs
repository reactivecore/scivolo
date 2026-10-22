//! The `String` of JDK members, which javalib declares as the alias `scala.Predef.String`: the
//! class `java.lang.String`, as dotty reads Java signatures.

use super::{SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// Type `t` of a member `def` of a `java.*` or `javax.*` class with its references to
    /// `scala.Predef.String` made to `java.lang.String`.
    pub fn with_java_strings(&mut self, def: SymbolId, t: TypeId) -> TypeId {
        let Some(owner) = self.symbol(def).owner else { return t };
        let class = self.full_name(owner);
        if !class.starts_with("java.") && !class.starts_with("javax.") {
            return t;
        }
        let mut string = None;
        self.map_type(t, &mut |u, part| {
            if !matches!(u.types().get(part), Type::Sym(..) | Type::TypeRef(..)) {
                return None;
            }
            let s = u.referenced(part)?;
            let predef = u.symbol(s).owner.is_some_and(|o| u.full_name(o) == "scala.Predef$");
            if u.symbol(s).name != "String" || !predef {
                return None;
            }
            Some(*string.get_or_insert_with(|| u.class_ref("java.lang.String")))
        })
    }
}
