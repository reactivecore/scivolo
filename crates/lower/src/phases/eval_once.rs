// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! dotty's `evalOnce`: a value bound to a fresh `ev$N` unless it is a stable path.

use crate::Lower;
use crate::definitions::symbol;
use crate::symbols::has;
use tasty::tags::{MUTABLE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Whether `t` is a literal, `this`, or a path of stable values and objects (dotty's
/// `isIdempotentExpr` for these).
pub fn is_stable_path(l: &mut Lower, t: TNodeId) -> bool {
    let stable = |l: &mut Lower, s: Option<SymbolId>| {
        s.is_some_and(|s| {
            l.u.is_module(s)
                || matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param)
                    && !has(l.u, s, MUTABLE)
                    && !super::first_transform::is_by_name(l, s)
        })
    };
    match *l.tpd.tree(t) {
        Tree::Literal { .. } | Tree::ClassLiteral { .. } | Tree::This { .. } => true,
        Tree::Ident { .. } => stable(l, l.tpd.sym(t)),
        Tree::Select { qualifier, .. } => stable(l, l.tpd.sym(t)) && is_stable_path(l, qualifier),
        _ => false,
    }
}

/// Whether `t` is a stable path, or `new C(args)` of a class without initialization code and
/// stable `args`, possibly ascribed (dotty's `isIdempotentExpr` for these).
pub fn is_idempotent(l: &mut Lower, t: TNodeId) -> bool {
    match *l.tpd.tree(t) {
        Tree::Typed { expr, .. } => is_idempotent(l, expr),
        Tree::Apply { mut fun, args } => {
            if let Tree::TypeApply { fun: f, .. } = *l.tpd.tree(fun) {
                fun = f;
            }
            let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return false };
            let Tree::New { tpt } = *l.tpd.tree(qualifier) else { return false };
            let class = l.u.class_symbol(l.tpd.tpe(tpt));
            let args = l.tpd.items(args).to_vec();
            class.is_some_and(|c| l.u.no_inits(c)) && args.into_iter().all(|a| is_idempotent(l, a))
        }
        _ => is_stable_path(l, t),
    }
}

/// `val ev$N = expr` in `owner` (dotty's `evalOnce`), with its symbol.
pub fn eval_once(l: &mut Lower, expr: TNodeId, owner: SymbolId) -> (SymbolId, TNodeId) {
    let tpe = l.u.widen(l.tpd.tpe(expr));
    eval_once_as(l, expr, owner, tpe)
}

/// `eval_once` with a value of type `tpe`.
pub fn eval_once_as(l: &mut Lower, expr: TNodeId, owner: SymbolId, tpe: TypeId) -> (SymbolId, TNodeId) {
    let name = l.fresh.next(l.names, "ev$");
    let name = l.names.text(name).to_owned();
    let ev = l.u.define(symbol(&name, Kind::Val, owner, Flags::default().with(SYNTHETIC)));
    l.u.set_info(ev, tpe);
    let span = l.tpd.span(expr);
    let tpt = l.type_tree(tpe, span);
    let term = l.term_name(&name);
    let def = l.tpd.node(Tree::ValDef { name: term, tpt, rhs: Some(expr) }, span, Some(tpe), Some(ev));
    (ev, def)
}
