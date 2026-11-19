// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! References to the type parameters of a class whose `this` a proxy stands for in an inline
//! expansion (dotty's `paramProxy` entries of `registerType`).

use super::Inlining;
use super::copy::TreeTypeMap;
use crate::tpd::{TNodeId, Tree as T};
use tasty::SymbolId;

/// An inferred type tree of the type `m` maps type tree `t` to, if `t` refers to a type parameter
/// of a class whose `this` a proxy of `this` stands for.
pub fn replace_class_tparam(
    i: &mut Inlining<'_>,
    m: &TreeTypeMap,
    this: &[(SymbolId, SymbolId)],
    t: TNodeId,
) -> Option<TNodeId> {
    let T::Ident { name } = *i.tpd.tree(t) else { return None };
    let s = i.tpd.sym(t)?;
    let owner = i.u.symbol(s).owner?;
    if !this.iter().any(|&(cls, _)| cls == owner) || !i.u.type_params(owner).contains(&s) {
        return None;
    }
    let tpe = i.map_type(m, i.tpd.tpe(t));
    if name.is_type {
        return Some(i.tpd.node(T::TypeTree { inferred: true }, i.tpd.span(t), Some(tpe), None));
    }
    let sym = i.u.sym_of(tpe).or_else(|| i.u.class_symbol(tpe));
    Some(i.tpd.node(T::Ident { name }, i.tpd.span(t), Some(tpe), sym))
}
