// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/staging/CrossStageSafety.scala
//   compiler/src/dotty/tools/dotc/staging/HealType.scala
//! The types of a quote's body with their aliases dealiased, as dotty's `CrossStageSafety` heals
//! them before the quote is pickled.

use super::{SPLICE, staged};
use crate::Lower;
use crate::phases::walk::children;
use rustc_hash::FxHashSet;
use tasty::{Type, TypeId, Universe};
use typer::tpd::{TNodeId, Tree};

/// Heals the trees of quote body `body` outside its splices, inner trees first: a type tree
/// whose type changes becomes a `TypeTree`, an applied type tree takes its parts' types, a term
/// reference and a definition keep theirs, any other tree and a splice get theirs healed.
pub fn heal(l: &mut Lower, body: TNodeId) {
    let (mut order, mut todo) = (Vec::new(), vec![body]);
    while let Some(t) = todo.pop() {
        order.push(t);
        if staged(l, t, SPLICE).is_none() {
            todo.extend(children(l.tpd, t));
        }
    }
    let mut changed = FxHashSet::default();
    for &t in order.iter().rev() {
        let Some(tpe) = l.tpd.get(t).tpe else { continue };
        let healed = match *l.tpd.tree(t) {
            _ if staged(l, t, SPLICE).is_some() => {
                let widened = l.u.widen(tpe);
                healed(l.u, widened)
            }
            Tree::Ident { name } if !name.is_type => continue,
            Tree::This { .. } | Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::Bind { .. } => continue,
            Tree::Ident { .. } | Tree::SingletonTypeTree { .. } => retyped(l, t, tpe),
            Tree::Select { name, .. } if name.is_type => retyped(l, t, tpe),
            Tree::AppliedTypeTree { tpt, args } => {
                let parts = std::iter::once(tpt).chain(l.tpd.items(args).iter().copied());
                if !parts.clone().any(|p| changed.contains(&p)) {
                    continue;
                }
                let parts: Vec<TypeId> = parts.map(|p| l.tpd.tpe(p)).collect();
                l.u.intern_type(Type::Applied(parts[0], parts[1..].to_vec()))
            }
            _ => healed(l.u, tpe),
        };
        if healed != tpe {
            l.tpd.get_mut(t).tpe = Some(healed);
            changed.insert(t);
        }
    }
}

/// The healed type of type tree `t` of type `tpe`, `t` made a `TypeTree` if it changes.
fn retyped(l: &mut Lower, t: TNodeId, tpe: TypeId) -> TypeId {
    let healed = healed(l.u, tpe);
    if healed != tpe {
        let node = l.tpd.get_mut(t);
        (node.tree, node.sym) = (Tree::TypeTree { inferred: false }, None);
    }
    healed
}

/// `t` with the aliases among its parts dealiased (dotty's `HealType` over a type without
/// references to earlier levels).
fn healed(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, x| match u.types().get(x) {
        Type::Sym(..) | Type::TypeRef(..) => u.referenced_alias(x).map(|a| healed(u, a)),
        _ => None,
    })
}
