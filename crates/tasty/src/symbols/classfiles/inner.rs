// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//! Inner classes of class files: Java classes nested in a class and not static.

use super::super::{Kind, SymbolId, Universe};
use super::signatures::Var;
use crate::NOTHING;
use crate::types::{Type, TypeId};

/// Determines whether `class` is an inner class: one nested in a class, not static.
pub fn is_inner(u: &Universe, class: SymbolId) -> bool {
    u.symbol(class).owner.is_some_and(|o| u.symbol(o).kind == Kind::Class && !u.is_module_class(o))
}

/// Determines the type of `class` without its type arguments, an inner class's seen from the
/// enclosing class's `this` (dotty's `typeRef`).
pub fn class_type(u: &mut Universe, class: SymbolId) -> TypeId {
    let owner = u.symbol(class).owner.filter(|_| is_inner(u, class));
    let prefix = owner.map(|o| {
        let outer = u.types.intern(Type::Sym(None, o));
        u.types.intern(Type::This(outer))
    });
    u.types.intern(Type::Sym(prefix, class))
}

/// Determines the type parameters of the classes enclosing inner class `class`, seen from their
/// `this` (dotty's `addEnclosingTParams`), outermost first.
pub fn enclosing_vars(u: &mut Universe, class: SymbolId) -> Vec<Var> {
    let mut vars = Vec::new();
    let mut c = class;
    while is_inner(u, c) {
        c = u.symbol(c).owner.expect("an inner class's owner");
        let this = u.types.intern(Type::Sym(None, c));
        let this = u.types.intern(Type::This(this));
        let from_java_object = u.types.intern(Type::Sym(None, u.defn().from_java_object));
        let object_bound = u.types.intern(Type::Bounds(NOTHING, from_java_object));
        for p in u.type_params(c).into_iter().rev() {
            let t = u.types.intern(Type::Sym(Some(this), p));
            let plain = u.infos.get(&p) == Some(&object_bound);
            vars.insert(0, (u.symbol(p).name.clone(), t, plain));
        }
    }
    vars
}
