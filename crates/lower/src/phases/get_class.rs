// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! `getClass` of a primitive value as its class constant (dotty's `constToLiteral` of
//! `pre.getClass()`).
#![allow(non_upper_case_globals)]

use super::first_transform::{is_literal, is_pure, literal, through};
use crate::Lower;
use rustc_hash::FxHashSet;
use tasty::tags::CLASSconst;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes `pre.getClass()` of a class constant type the constant, after `pre` unless `pre` is a
/// literal or a pure path not through objects `inits`; whether `t` is such a call.
pub fn fold(l: &mut Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let Some(pre) = receiver(l, t) else { return false };
    if !matches!(l.u.types().get(l.tpd.tpe(t)), Type::Constant(CLASSconst, _)) {
        return false;
    }
    let Some(lit) = literal(l.u.types().get(l.tpd.tpe(t))) else { return false };
    let mut inner = pre;
    while let Tree::Typed { expr, .. } = *l.tpd.tree(inner) {
        inner = expr;
    }
    let pure = is_literal(l, inner) || is_pure(l, inner) && !through(l, inner, inits);
    let tree = if pure {
        lit
    } else {
        let (span, tpe) = (l.tpd.span(t), l.tpd.tpe(t));
        let expr = l.tpd.node(lit, span, Some(tpe), None);
        Tree::Block { stats: l.tpd.list(&[pre]), expr }
    };
    let node = l.tpd.get_mut(t);
    (node.tree, node.sym) = (tree, None);
    true
}

/// `pre` if `t` is `pre.getClass()` of `Any`.
fn receiver(l: &Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return None };
    let Tree::TypeApply { fun: select, .. } = *l.tpd.tree(fun) else { return None };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(select) else { return None };
    let s = l.tpd.sym(select)?;
    let any_get_class = l.u.symbol(s).name == "getClass" && l.u.symbol(s).owner == Some(l.u.builtins().any);
    (any_get_class && l.tpd.items(args).is_empty()).then_some(qualifier)
}
