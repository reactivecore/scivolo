// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeTypeMap.scala
//! Copies of typed trees whose local definitions get fresh symbols, with symbols substituted and
//! an owner changed (dotty's `TreeTypeMap` of `subst` and `changeOwner`).

use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::{Symbol, SymbolId, Type, TypeId};
use typer::tpd::{self, TNodeId, Tree};

/// The symbols a copy replaces, and the owner whose definitions `to` owns in it.
pub struct Copier {
    pub syms: FxHashMap<SymbolId, SymbolId>,
    pub from: SymbolId,
    pub to: SymbolId,
}

/// A copy of `t` through `m`; refused if `t` defines a class or type.
pub fn copy(l: &mut Lower, m: &mut Copier, t: TNodeId) -> Result<TNodeId> {
    let mut fresh = Vec::new();
    for s in subtrees(l.tpd, t) {
        match l.tpd.tree(s) {
            Tree::TypeDef { .. } | Tree::Template { .. } => {
                return unsupported(l.tpd.span(s), "partial functions defining classes or types");
            }
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::Bind { .. } => {
                if let Some(d) = l.tpd.sym(s) {
                    fresh.push((d, fresh_symbol(l, m, d)));
                }
            }
            _ => {}
        }
    }
    for (old, new) in fresh {
        let info = l.u.info(old);
        let info = map_type(l, m, info);
        l.u.set_info(new, info);
    }
    Ok(copy_node(l, m, t))
}

fn copy_node(l: &mut Lower, m: &Copier, t: TNodeId) -> TNodeId {
    let node = l.tpd.get(t);
    let (tree, span, tpe, sym) = (node.tree.clone(), node.span, node.tpe, node.sym);
    let tpe = tpe.map(|t| map_type(l, m, t));
    let sym = sym.map(|s| m.syms.get(&s).copied().unwrap_or(s));
    let copy = l.tpd.node(tree, span, tpe, sym);
    if let Some(&at) = l.inlined_spans.get(&t) {
        l.inlined_spans.insert(copy, at);
    }
    if l.inlined_blocks.contains(&t) {
        l.inlined_blocks.insert(copy);
    }
    tpd::map_children(l, copy, &mut |l, c| copy_node(l, m, c));
    copy
}

fn map_type(l: &mut Lower, m: &Copier, t: TypeId) -> TypeId {
    l.u.map_type(t, &mut |u, part| match *u.types().get(part) {
        Type::Sym(None, s) => m.syms.get(&s).map(|&n| u.intern_type(Type::Sym(None, n))),
        _ => None,
    })
}

/// A fresh symbol for local definition `s`, owned by what replaces its owner.
fn fresh_symbol(l: &mut Lower, m: &mut Copier, s: SymbolId) -> SymbolId {
    let old = l.u.symbol(s);
    let owner = old.owner.map(|o| match m.syms.get(&o) {
        Some(&n) => n,
        None if o == m.from => m.to,
        None => o,
    });
    let fresh = Symbol {
        name: old.name.clone(),
        kind: old.kind,
        owner,
        flags: old.flags,
        target: None,
        is_extern: false,
        derived: old.derived,
        decls: Vec::new(),
        tree: None,
    };
    let n = l.u.define(fresh);
    m.syms.insert(s, n);
    n
}
