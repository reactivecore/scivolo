//! What the lowering asks of symbols (dotty's `SymDenotations` predicates).

use tasty::tags::{OBJECT, PRIVATE, STATIC, TRAIT};
use tasty::{Kind, SymbolId, Universe};

pub fn has(u: &Universe, s: SymbolId, tag: u8) -> bool {
    u.symbol(s).flags.has(tag)
}

pub fn is_class(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).kind == Kind::Class
}

pub fn is_module_class(u: &Universe, s: SymbolId) -> bool {
    is_class(u, s) && has(u, s, OBJECT)
}

/// A module value, not its class.
pub fn is_module(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).kind.is_term() && has(u, s, OBJECT)
}

pub fn is_trait(u: &Universe, s: SymbolId) -> bool {
    is_class(u, s) && has(u, s, TRAIT)
}

pub fn is_private(u: &Universe, s: SymbolId) -> bool {
    has(u, s, PRIVATE)
}

/// Java's `static`, which `@main` proxies carry.
pub fn is_java_static(u: &Universe, s: SymbolId) -> bool {
    has(u, s, STATIC)
}

pub fn is_constructor(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).kind == Kind::Def && u.symbol(s).name == "<init>"
}

/// Whether `s` is a package or an object reachable through static paths.
pub fn is_static_owner(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).kind == Kind::Package || (is_module_class(u, s) && is_static(u, s))
}

/// Whether the owner of `s` is a static owner.
pub fn is_static(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).owner.is_some_and(|o| is_static_owner(u, o))
}

pub fn is_static_module(u: &Universe, s: SymbolId) -> bool {
    is_module_class(u, s) && is_static(u, s)
}

/// The class declared beside module class `c` under its name without `$`.
pub fn companion_class(u: &mut Universe, c: SymbolId) -> Option<SymbolId> {
    let owner = u.symbol(c).owner?;
    let name = u.symbol(c).name.strip_suffix('$')?.to_owned();
    let found = u.decl(owner, &name).ok()?;
    found.into_iter().find(|&s| is_class(u, s) && !has(u, s, OBJECT))
}

/// The object declared beside class `c`, its value.
pub fn companion_module(u: &mut Universe, c: SymbolId) -> Option<SymbolId> {
    let owner = u.symbol(c).owner?;
    let name = u.symbol(c).name.clone();
    let found = u.decl(owner, &name).ok()?;
    found.into_iter().find(|&s| is_module(u, s))
}

/// The value of module class `c`.
pub fn source_module(u: &mut Universe, c: SymbolId) -> Option<SymbolId> {
    let owner = u.symbol(c).owner?;
    let name = u.symbol(c).name.strip_suffix('$')?.to_owned();
    let found = u.decl(owner, &name).ok()?;
    found.into_iter().find(|&s| is_module(u, s))
}
