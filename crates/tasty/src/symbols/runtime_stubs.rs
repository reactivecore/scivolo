//! Java classes of `scala.runtime` without TASTy: `Statics`, the `*Ref` and `Volatile*Ref`
//! classes of the primitive types, and `BoxedUnit`. A Java class's static members are those of
//! its object, its fields values.

use super::any_members::{class_type, method};
use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::tags::{FINAL, MUTABLE, OBJECT, STATIC};
use crate::types::{Type, TypeId};

/// The primitive types with a `*Ref` class.
const REFS: [&str; 8] = ["Boolean", "Byte", "Char", "Double", "Float", "Int", "Long", "Short"];

pub fn enter(u: &mut Universe, object: SymbolId) {
    let Some(runtime) = u.package("scala.runtime") else { return };
    let object = u.types.intern(Type::Sym(None, object));
    let int = class_type(u, "scala", "Int");
    if let Some((_, statics)) = java_class(u, runtime, "Statics", object) {
        let long = class_type(u, "scala", "Long");
        let float = class_type(u, "scala", "Float");
        let double = class_type(u, "scala", "Double");
        let unit = class_type(u, "scala", "Unit");
        let two = [("x$0", int), ("x$1", int)];
        for name in ["mix", "mixLast", "finalizeHash"] {
            method(u, statics, name, &two, int, &[]);
        }
        method(u, statics, "avalanche", &[("x$0", int)], int, &[]);
        method(u, statics, "longHash", &[("x$0", long)], int, &[]);
        method(u, statics, "doubleHash", &[("x$0", double)], int, &[]);
        method(u, statics, "floatHash", &[("x$0", float)], int, &[]);
        method(u, statics, "anyHash", &[("x$0", object)], int, &[]);
        method(u, statics, "releaseFence", &[], unit, &[]);
    }
    if let Some((class, module)) = java_class(u, runtime, "BoxedUnit", object) {
        let unit = synthetic(u, module, "UNIT", Kind::Val, &[STATIC, FINAL]);
        let info = u.types.intern(Type::Sym(None, class));
        u.infos.insert(unit, info);
    }
    let string = class_type(u, "java.lang", "String");
    for prefix in ["", "Volatile"] {
        for primitive in REFS {
            let name = format!("{prefix}{primitive}Ref");
            let Some((class, module)) = java_class(u, runtime, &name, object) else { continue };
            let elem = class_type(u, "scala", primitive);
            let this = u.types.intern(Type::Sym(None, class));
            let field = synthetic(u, class, "elem", Kind::Val, &[MUTABLE]);
            u.infos.insert(field, elem);
            method(u, class, "<init>", &[("x$0", elem)], this, &[]);
            method(u, class, "toString", &[], string, &[]);
            method(u, module, "create", &[("x$0", elem)], this, &[]);
            method(u, module, "zero", &[], this, &[]);
        }
    }
}

/// Java class `name` of package `pkg` extending `Object` and the class of its object, holding the
/// static members; none if the library has its TASTy.
fn java_class(u: &mut Universe, pkg: SymbolId, name: &str, object: TypeId) -> Option<(SymbolId, SymbolId)> {
    if u.pending.get(&pkg).is_some_and(|f| f.iter().any(|(stem, _)| stem == name)) {
        return None;
    }
    let class = synthetic(u, pkg, name, Kind::Class, &[]);
    u.parents.insert(class, vec![object]);
    let module_class = synthetic(u, pkg, &format!("{name}$"), Kind::Class, &[OBJECT, FINAL]);
    u.parents.insert(module_class, vec![object]);
    let module = synthetic(u, pkg, name, Kind::Val, &[OBJECT, FINAL]);
    let info = u.types.intern(Type::Sym(None, module_class));
    u.infos.insert(module, info);
    u.java_stubs.extend([class, module_class, module]);
    Some((class, module_class))
}

impl Universe {
    /// Whether `c` is the class of the object of a Java class entered here, whose members are
    /// static.
    pub fn is_java_statics(&self, c: SymbolId) -> bool {
        self.is_module_class(c) && self.is_java_stub(c)
    }

    /// Whether `s` is an instance field of a Java class entered here.
    pub fn is_java_field(&self, s: SymbolId) -> bool {
        self.symbol(s).kind == Kind::Val
            && !self.is_module(s)
            && !self.symbol(s).flags.has(STATIC)
            && self.symbol(s).owner.is_some_and(|c| self.is_java_stub(c))
    }

    fn is_java_stub(&self, c: SymbolId) -> bool {
        self.symbol(c).kind == Kind::Class && self.is_library_java_stub(c)
    }

    /// Whether `s` is a Java class of the Scala library entered here, its object or the object's
    /// class.
    pub fn is_library_java_stub(&self, s: SymbolId) -> bool {
        self.java_stubs.contains(&s)
    }
}
