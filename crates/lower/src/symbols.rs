//! What the lowering asks of symbols (dotty's `SymDenotations` predicates).

use tasty::tags::{FINAL, PRIVATE, STATIC};
use tasty::{Kind, SymbolId, Universe};

pub fn has(u: &Universe, s: SymbolId, tag: u8) -> bool {
    u.symbol(s).flags.has(tag)
}

pub fn is_class(u: &Universe, s: SymbolId) -> bool {
    u.is_class(s)
}

pub fn is_module_class(u: &Universe, s: SymbolId) -> bool {
    u.is_module_class(s)
}

pub fn is_module(u: &Universe, s: SymbolId) -> bool {
    u.is_module(s)
}

pub fn is_trait(u: &Universe, s: SymbolId) -> bool {
    u.is_trait(s)
}

pub fn is_private(u: &Universe, s: SymbolId) -> bool {
    has(u, s, PRIVATE)
}

/// Whether `tpe` is the type of a single value or an object's class, which `null` does not
/// conform to (dotty's `isNullableClass`).
pub fn is_singleton(u: &mut Universe, tpe: tasty::TypeId) -> bool {
    match *u.types().get(tpe) {
        tasty::Type::Sym(_, s) if u.symbol(s).kind.is_term() => true,
        tasty::Type::TermRef(..) => true,
        _ => u.class_symbol(tpe).is_some_and(|c| u.is_module_class(c)),
    }
}

/// Whether member `s` of class `class` cannot be overridden.
pub fn is_effectively_final(u: &Universe, class: SymbolId, s: SymbolId) -> bool {
    has(u, s, PRIVATE) || has(u, s, FINAL) || has(u, class, FINAL) || u.is_module_class(class)
}

/// Java's `static`, which `@main` proxies carry, and the methods of javalib's objects that
/// stand for the static members of JDK classes.
pub fn is_java_static(u: &Universe, s: SymbolId) -> bool {
    has(u, s, STATIC)
        || (u.symbol(s).kind == Kind::Def && u.symbol(s).owner.is_some_and(|o| is_jdk_object(u, o)))
}

/// Whether class `c` is the class of a javalib object in a JDK package.
fn is_jdk_object(u: &Universe, c: SymbolId) -> bool {
    let name = u.full_name(c);
    u.is_module_class(c) && (name.starts_with("java.") || name.starts_with("javax."))
}

pub fn is_constructor(u: &Universe, s: SymbolId) -> bool {
    u.is_constructor(s)
}

/// Whether `s` is a package or an object reachable through static paths.
pub fn is_static_owner(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).kind == Kind::Package || is_static_module(u, s)
}

/// Whether the owner of `s` is a static owner.
pub fn is_static(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).owner.is_some_and(|o| is_static_owner(u, o))
}

/// Whether `s` or its owner is Scala Native's `@extern`.
pub fn is_extern(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).is_extern || u.symbol(s).owner.is_some_and(|o| u.symbol(o).is_extern)
}

pub fn is_static_module(u: &Universe, s: SymbolId) -> bool {
    u.is_module_class(s) && is_static(u, s)
}
