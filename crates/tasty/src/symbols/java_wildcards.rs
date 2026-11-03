// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//! The wildcards of JDK members, which javalib and the JDK's declarations write `?`: bounded by
//! `<FromJavaObject>`, as dotty reads Java signatures.

use super::{ANY, SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// Type `t` of member `def` of a Java class of a `java.*` or `javax.*` package with the upper
    /// bound `Any` of its type arguments made `<FromJavaObject>`, but for those of arrays, which
    /// Java has no wildcards for.
    pub fn with_java_wildcards(&mut self, def: SymbolId, t: TypeId) -> TypeId {
        let Some(owner) = self.symbol(def).owner else { return t };
        let class = self.full_name(owner);
        if !(class.starts_with("java.") || class.starts_with("javax.")) || !self.may_be_java(owner) {
            return t;
        }
        let from_java_object = self.intern_type(Type::Sym(None, self.builtins().from_java_object));
        self.java_wildcards(t, from_java_object)
    }

    fn java_wildcards(&mut self, t: TypeId, from_java_object: TypeId) -> TypeId {
        self.map_type(t, &mut |u, part| {
            let Type::Applied(tycon, args) = u.types().get(part).clone() else { return None };
            if u.class_symbol(tycon).is_some_and(|c| u.full_name(c) == "scala.Array") {
                return None;
            }
            let any = u.builtins().any;
            let args = args.into_iter().map(|a| match u.types().get(a).clone() {
                Type::Bounds(lo, hi) if hi == ANY || u.referenced(hi) == Some(any) => {
                    let lo = u.java_wildcards(lo, from_java_object);
                    u.intern_type(Type::Bounds(lo, from_java_object))
                }
                _ => u.java_wildcards(a, from_java_object),
            });
            let args = args.collect();
            Some(u.intern_type(Type::Applied(tycon, args)))
        })
    }
}
