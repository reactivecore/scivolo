// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/CollectNullableFields.scala
//! dotty's `CollectNullableFields`: the `private[this]` fields of reference types that only one
//! lazy value of their class reads, which `LazyVals` sets to `null` once the value is computed.

use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashMap;
use tasty::tags::{ERASED, FIELDaccessor, LAZY, LOCAL, PRIVATE};
use tasty::{Erased, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree, Walk, traverse};

/// The fields under `root`, owned by `owner`, each lazy value nulls, in the order of their first
/// use.
pub fn collect(l: &mut Lower, root: TNodeId, owner: SymbolId) -> FxHashMap<SymbolId, Vec<SymbolId>> {
    let mut uses: Vec<(SymbolId, Option<SymbolId>)> = Vec::new();
    traverse(l, root, owner, &mut |l, t, owner| {
        if let (Tree::Ident { .. } | Tree::Select { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t))
            && is_nullable_private_field(l, s)
        {
            match uses.iter_mut().find(|(f, _)| *f == s) {
                Some((_, by)) if *by != Some(owner) => *by = None,
                Some(_) => {}
                None => {
                    let lazy = has(l.u, owner, LAZY) && !l.u.is_module(owner);
                    let by = (lazy && l.u.symbol(owner).owner == l.u.symbol(s).owner).then_some(owner);
                    uses.push((s, by));
                }
            }
        }
        Walk::Children
    });
    let mut out: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
    for (field, by) in uses {
        if let Some(lazy) = by {
            out.entry(lazy).or_default().push(field);
        }
    }
    out
}

/// Whether `s` is a `private[this]` field of a class but a trait, not lazy, of a type whose
/// erasure admits `null`.
fn is_nullable_private_field(l: &mut Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    let in_class = sym.owner.is_some_and(|o| l.u.is_class(o) && !l.u.is_trait(o));
    if sym.kind != Kind::Val || !in_class || [LAZY, ERASED, FIELDaccessor].iter().any(|&f| has(l.u, s, f)) {
        return false;
    }
    if !has(l.u, s, PRIVATE) || !has(l.u, s, LOCAL) {
        return false;
    }
    let info = l.u.info(s);
    if let Type::ByName(_) = *l.u.types().get(info) {
        return true;
    }
    let tpe = l.u.widen(info);
    let tpe = l.u.dealias(tpe);
    let head = match *l.u.types().get(tpe) {
        Type::Applied(f, _) => f,
        _ => tpe,
    };
    let class = match *l.u.types().get(head) {
        Type::Sym(_, c) => c,
        Type::TypeRef(..) => {
            let Some(c) = l.u.resolve_ref(head) else { return false };
            c
        }
        _ => return false,
    };
    let reference = !matches!(l.u.erase(tpe), Erased::Prim(_) | Erased::Unit | Erased::Nothing);
    l.u.symbol(class).kind == Kind::Class
        && reference
        && l.u.value_class(tpe).is_none()
        && !l.u.is_module_class(class)
}
