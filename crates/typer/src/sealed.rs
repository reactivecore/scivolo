// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//! The children of sealed classes and the shape of generic products, as the typer's mirrors and
//! the lowering's mirror support both see them (dotty's `children`, `isGenericProduct`).

use tasty::tags::{ABSTRACT, CASE, ENUM, OBJECT};
use tasty::{Kind, SymbolId, Type, Universe};

/// The cases of enum class `e`, in the order of their definitions.
pub fn enum_cases(u: &mut Universe, e: SymbolId) -> Vec<SymbolId> {
    let companion = u.companion_module(e).and_then(|m| u.module_class(m));
    let decls = companion.map(|c| u.symbol(c).decls.clone()).unwrap_or_default();
    decls
        .into_iter()
        .filter(|&d| {
            let (kind, flags) = (u.symbol(d).kind, u.symbol(d).flags);
            let case = kind == Kind::Class && !flags.has(OBJECT) || kind == Kind::Val && !flags.has(OBJECT);
            case && flags.has(CASE) && flags.has(ENUM)
        })
        .collect()
}

/// The children of sealed class `s` among `classes`, completed classes each with the start of its
/// definition: those with `s` as a parent, objects by their values, in the order of definitions.
pub fn direct_children(u: &mut Universe, s: SymbolId, classes: &[(u32, SymbolId)]) -> Vec<SymbolId> {
    let mut found: Vec<(u32, SymbolId)> = Vec::new();
    for &(start, c) in classes {
        if c == s || !u.parents(c).into_iter().any(|p| u.class_symbol(p) == Some(s)) {
            continue;
        }
        let module = u.is_module_class(c).then(|| u.source_module(c)).flatten();
        found.push((start, module.unwrap_or(c)));
    }
    found.sort_by_key(|&(start, _)| start);
    found.into_iter().map(|(_, c)| c).collect()
}

/// Whether `owner`, where a child of `s` is defined, encloses `s` or is an object visible so.
pub fn visible_to_parent(u: &Universe, s: SymbolId, owner: SymbolId) -> bool {
    is_contained_in(u, s, owner)
        || u.is_module_class(owner) && u.symbol(owner).owner.is_some_and(|o| visible_to_parent(u, s, o))
}

/// Whether `s` is `owner` or owned by it, transitively.
pub fn is_contained_in(u: &Universe, s: SymbolId, owner: SymbolId) -> bool {
    std::iter::successors(Some(s), |&c| u.symbol(c).owner).any(|c| c == owner)
}

/// Whether completed class `c` is a case class of a generic product's shape: not abstract, one
/// parameter list, no value class (`value_class`).
pub fn is_product_shape(u: &mut Universe, c: SymbolId, value_class: bool) -> bool {
    let flags = u.symbol(c).flags;
    flags.has(CASE) && !flags.has(ABSTRACT) && param_lists(u, c) == 1 && !value_class
}

/// The upper bound of type parameter `p`.
pub fn upper_bound(u: &mut Universe, p: SymbolId) -> tasty::TypeId {
    let info = u.info(p);
    match *u.types().get(info) {
        Type::Bounds(_, hi) => hi,
        _ => info,
    }
}

/// The number of parameter lists of the primary constructor of class `c`.
fn param_lists(u: &mut Universe, c: SymbolId) -> usize {
    let Some(&ctor) = u.decl(c, "<init>").unwrap_or_default().first() else { return 0 };
    let mut t = u.info(ctor);
    let mut lists = 0;
    loop {
        match u.types().get(t) {
            Type::Poly { result, .. } => t = *result,
            Type::Method { result, .. } => (t, lists) = (*result, lists + 1),
            _ => return lists,
        }
    }
}
