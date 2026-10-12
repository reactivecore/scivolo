//! dotty's `VCElideAllocations` on the trees `Erasure` types: `new C(u).value()` is `u`, and
//! `new C(a) == new C(b)` is `a == b` unless `C` has an `equals` of its own.

use super::adapt::erased_type;
use super::casts::class_named;
use crate::Lower;
use crate::definitions::primitive_class_name;
use crate::symbols::has;
use tasty::tags::SYNTHETIC;
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// `t` without the allocation of a value class it undoes at once.
pub fn elided(l: &mut Lower, t: TNodeId) -> TNodeId {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return t };
    let (Tree::Select { qualifier, .. }, Some(m)) = (l.tpd.tree(fun), l.tpd.sym(fun)) else { return t };
    let Some((class, u)) = new_with_arg(l, *qualifier) else { return t };
    let args = l.tpd.items(args).to_vec();
    if args.is_empty() && l.u.underlying(class) == Some(m) {
        return u;
    }
    let [other] = args[..] else { return t };
    match new_with_arg(l, other) {
        Some((c, v)) if c == class && m == l.defs.patmat.any_eq && !has_own_equals(l, class) => {
            let span = l.tpd.span(t);
            let eq = equal_values(l, u, v);
            l.tpd.get_mut(eq).span = span;
            eq
        }
        _ => t,
    }
}

/// The class and argument of `new C(u)`.
fn new_with_arg(l: &Lower, t: TNodeId) -> Option<(SymbolId, TNodeId)> {
    let Tree::Apply { mut fun, args } = *l.tpd.tree(t) else { return None };
    if let Tree::TypeApply { fun: inner, .. } = *l.tpd.tree(fun) {
        fun = inner;
    }
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return None };
    let ctor = l.tpd.sym(fun).filter(|&c| l.u.is_constructor(c))?;
    if !matches!(l.tpd.tree(qualifier), Tree::New { .. }) {
        return None;
    }
    let &[u] = l.tpd.items(args) else { return None };
    Some((l.u.symbol(ctor).owner?, u))
}

/// Whether value class `class` has or inherits an `equals` it did not get synthesized.
fn has_own_equals(l: &mut Lower, class: SymbolId) -> bool {
    let plain = [l.defs.any, l.defs.object];
    l.u.member(class, "equals").into_iter().any(|m| {
        let s = l.u.symbol(m);
        s.kind == Kind::Def && !s.owner.is_some_and(|o| plain.contains(&o)) && !has(l.u, m, SYNTHETIC)
    })
}

/// `a == b` with the `==` of `a`'s erased class taking `b`'s erased type, else `Any`'s (dotty's
/// `applyOverloaded`).
fn equal_values(l: &mut Lower, a: TNodeId, b: TNodeId) -> TNodeId {
    let (ea, eb) = (erased_type(l, a).full(), erased_type(l, b).full());
    let name = match ea {
        Erased::Prim(c) => primitive_class_name(c),
        Erased::Class(name) => Some(name),
        _ => None,
    };
    let class = name.and_then(|n| class_named(l, &n));
    let ops = class.and_then(|c| l.u.decl(c, "==").ok()).unwrap_or_default();
    let op = ops.into_iter().find(|&m| l.u.erased_method(m).0 == [eb.clone()]);
    l.call(a, op.unwrap_or(l.defs.patmat.any_eq), &[b])
}
