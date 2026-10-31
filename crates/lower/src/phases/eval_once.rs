// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! dotty's `evalOnce`: a value bound to a fresh `ev$N` unless it is a stable path.

use super::companion_order::tree_offset;
use super::walk::subtrees;
use crate::Lower;
use crate::definitions::symbol;
use crate::symbols::has;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Reverse;
use tasty::tags::{MUTABLE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Whether `t` is a literal, `this`, or a path of stable values and objects, their getters
/// called (dotty's `isIdempotentExpr` for these).
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
        Tree::Apply { fun, args } if l.tpd.items(args).is_empty() => {
            matches!(l.tpd.tree(fun), Tree::Select { .. }) && is_stable_path(l, fun)
        }
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
            let Tree::New { tpt } = *l.tpd.tree(qualifier) else { return is_stable_path(l, t) };
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
    l.evaluated_once.push((ev, span, l.erasure_owner.is_some()));
    (ev, def)
}

/// Gives the values evaluated once in unit `root` since the last call their `ev$N` numbers in
/// the order dotty's phases make them: `Erasure`'s after the others, each in the order of the
/// trees, inner trees first, a value at the tree its block yields; a value without a span after
/// the one made before it.
pub fn renumber(l: &mut Lower, root: TNodeId) {
    let values = std::mem::take(&mut l.evaluated_once);
    let syms: FxHashSet<SymbolId> = values.iter().map(|&(v, ..)| v).collect();
    let mut at = FxHashMap::default();
    for t in subtrees(l.tpd, root) {
        let Tree::Block { stats, expr } = *l.tpd.tree(t) else { continue };
        let span = l.tpd.span(expr);
        for &s in l.tpd.items(stats) {
            at.extend(l.tpd.sym(s).filter(|v| span.exists() && syms.contains(v)).map(|v| (v, span)));
        }
    }
    let number = |n: &str| n.trim_start_matches("ev$").parse::<u32>().unwrap_or(0);
    let mut names: Vec<String> = values.iter().map(|&(v, ..)| l.u.symbol(v).name.clone()).collect();
    names.sort_by_key(|n| number(n));
    let mut key = (false, 0, Reverse(0));
    let mut keyed: Vec<_> = values
        .into_iter()
        .map(|(v, span, erasure)| {
            let span = at.get(&v).copied().unwrap_or(span);
            if span.exists() {
                let at = |o| tree_offset(l, o, Some(v));
                key = (erasure, at(span.end()), Reverse(at(span.start())));
            }
            (key, v)
        })
        .collect();
    keyed.sort_by_key(|&(key, _)| key);
    for ((_, v), name) in keyed.into_iter().zip(names) {
        l.u.rename(v, &name);
    }
}
