// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! The children of a sealed class seen through the prefix its sum mirror is summoned at
//! (dotty's `whyNotGenericSum` scope check and `TypeOps.childPrefix`).

use crate::sealed;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// Collects the classes of prefix `pre` if it is a singleton type, those of both sides of an
/// intersection (dotty's `ownerScope` of `classSymbols`).
pub fn owner_scope(u: &mut Universe, pre: Option<TypeId>) -> Vec<SymbolId> {
    let Some(p) = pre else { return Vec::new() };
    let singleton = match *u.types().get(p) {
        Type::This(_) => true,
        Type::Sym(_, s) => u.symbol(s).kind.is_term(),
        _ => false,
    };
    let mut todo = if singleton { vec![u.widen(p)] } else { Vec::new() };
    let mut classes = Vec::new();
    while let Some(t) = todo.pop() {
        match *u.types().get(t) {
            Type::And(a, b) => todo.extend([b, a]),
            _ => classes.extend(u.class_symbol(t)),
        }
    }
    classes
}

/// Whether `owner` encloses `site`, is inherited by a class of `scope`, or is a package or an
/// object visible so (dotty's `isVisibleToScope`).
pub fn is_visible_to_scope(u: &mut Universe, owner: SymbolId, site: SymbolId, scope: &[SymbolId]) -> bool {
    u.symbol(owner).kind == Kind::Package
        || sealed::is_contained_in(u, site, owner)
        || scope.iter().any(|&c| inherits(u, owner, c))
        || u.is_module_class(owner)
            && u.symbol(owner).owner.is_some_and(|o| is_visible_to_scope(u, o, site, scope))
}

/// Whether class `scope` or one of its owners up to a package derives from `sym`.
fn inherits(u: &mut Universe, sym: SymbolId, scope: SymbolId) -> bool {
    u.symbol(scope).kind != Kind::Package
        && (u.is_class(scope) && u.derives_from(scope, sym)
            || u.symbol(scope).owner.is_some_and(|o| inherits(u, sym, o)))
}

/// Determines the prefix of child `child` of sealed class `parent` referred to through `pre`: the
/// part of `pre` the owners of both share, with the child's other owners selected from it.
pub fn child_prefix(u: &mut Universe, pre: TypeId, parent: SymbolId, child: SymbolId) -> Option<TypeId> {
    let owners = |u: &Universe, s: SymbolId| -> Vec<SymbolId> {
        let mut v: Vec<SymbolId> = std::iter::successors(u.symbol(s).owner, |&o| u.symbol(o).owner).collect();
        v.reverse();
        v
    };
    let (parents, children) = (owners(u, parent), owners(u, child));
    let common = parents.iter().zip(&children).take_while(|(a, b)| a == b).count();
    let mut prefix = sub_prefix_at(u, pre, parents.len() - common)?;
    for &s in &children[common..] {
        let s = match u.is_module_class(s) {
            true => u.source_module(s)?,
            false => s,
        };
        let p = (!matches!(u.types().get(prefix), Type::NoPrefix)).then_some(prefix);
        prefix = u.intern_type(Type::Sym(p, s));
    }
    Some(prefix)
}

/// The prefix `limit` selections out of `pre`; `None` for a prefix of another form.
fn sub_prefix_at(u: &mut Universe, pre: TypeId, limit: usize) -> Option<TypeId> {
    if limit == 0 {
        return Some(pre);
    }
    match *u.types().get(pre) {
        Type::Sym(Some(p), s) if u.symbol(s).kind.is_term() => sub_prefix_at(u, p, limit - 1),
        Type::Sym(None, s) if u.symbol(s).kind.is_term() && limit == 1 => Some(u.intern_type(Type::NoPrefix)),
        _ => None,
    }
}
