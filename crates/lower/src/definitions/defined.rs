//! Symbols defined outside TASTy: Java classes and statics the library has no TASTy for.

use super::{member, type_ref};
use tasty::tags::STATIC;
use tasty::{Flags, Kind, Symbol, SymbolId, TypeId, Universe};

/// `BoxedUnit.UNIT`: the static of the Java class's object if entered, else defined with its class
/// if the library has no TASTy for them.
pub fn boxed_unit_unit(u: &mut Universe, runtime: SymbolId) -> SymbolId {
    if let Ok(statics) = member(u, runtime, "BoxedUnit$", Kind::Class)
        && u.is_java_statics(statics)
        && let Ok(unit) = member(u, statics, "UNIT", Kind::Val)
    {
        return unit;
    }
    let class = java_class(u, runtime, "BoxedUnit");
    let info = type_ref(u, class);
    java_static(u, class, "UNIT", Kind::Val, info)
}

/// Class `name` of package `owner`, defined if the library has no TASTy for it.
pub fn java_class(u: &mut Universe, owner: SymbolId, name: &str) -> SymbolId {
    match member(u, owner, name, Kind::Class) {
        Ok(c) => c,
        Err(_) => u.define(symbol(name, Kind::Class, owner, Flags::default())),
    }
}

/// Static member `name` of Java class `class`, defined of type `info` if missing.
pub fn java_static(u: &mut Universe, class: SymbolId, name: &str, kind: Kind, info: TypeId) -> SymbolId {
    if let Ok(s) = member(u, class, name, kind) {
        return s;
    }
    let s = u.define(symbol(name, kind, class, Flags::default().with(STATIC)));
    u.set_info(s, info);
    s
}

/// A symbol defined outside TASTy.
pub fn symbol(name: &str, kind: Kind, owner: SymbolId, flags: Flags) -> Symbol {
    Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags,
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    }
}
