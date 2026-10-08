//! dotty's `EtaReduce`: `{ def $anonfun(x1, ..., xN) = f.apply(x1, ..., xN); closure($anonfun) }`
//! becomes `f` if `f` is a pure path of a function type conforming to the closure's and no `xi`
//! is primitive, which erasure would box in the call.

use super::casts::conforms;
use super::first_transform::is_pure;
use super::walk::subtrees;
use crate::Lower;
use tasty::{Erased, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Reduces the eta expansions in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) {
    for t in subtrees(l.tpd, td) {
        if let Some(f) = reduced(l, t) {
            l.copy_node(t, f);
        }
    }
}

/// The function `t` eta-expands, if it is a block of a lambda's definition and its closure.
fn reduced(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    let &[def] = l.tpd.items(stats) else { return None };
    let (Tree::DefDef { paramss, rhs: Some(rhs), .. }, Some(meth)) =
        (l.tpd.tree(def).clone(), l.tpd.sym(def))
    else {
        return None;
    };
    let Tree::Closure { meth: closure_meth, .. } = *l.tpd.tree(expr) else { return None };
    if l.tpd.sym(closure_meth) != Some(meth) || l.u.symbol(meth).name != "$anonfun" {
        return None;
    }
    let Tree::Apply { fun, args } = *l.tpd.tree(rhs) else { return None };
    let Tree::Select { qualifier: f, .. } = *l.tpd.tree(fun) else { return None };
    if l.tpd.sym(fun).is_none_or(|m| l.u.symbol(m).name != "apply") {
        return None;
    }
    let params: Vec<Option<SymbolId>> =
        l.tpd.clause_items(paramss).iter().flat_map(|&c| l.tpd.items(c)).map(|&p| l.tpd.sym(p)).collect();
    let args: Vec<Option<SymbolId>> = l
        .tpd
        .items(args)
        .iter()
        .map(|&a| match l.tpd.tree(a) {
            Tree::Ident { .. } => l.tpd.sym(a),
            _ => None,
        })
        .collect();
    let own_param = l.tpd.sym(f).is_some_and(|s| l.u.symbol(s).owner == Some(meth));
    if params != args || params.contains(&None) || own_param || !is_pure(l, f) {
        return None;
    }
    let boxed = params.iter().flatten().any(|&p| {
        let info = l.u.info(p);
        matches!(l.u.erase(info), Erased::Prim(_) | Erased::Unit)
    });
    if boxed {
        return None;
    }
    let from = function_erasure(l, f)?;
    let to = l.u.erase(l.tpd.tpe(t));
    conforms(l, &from, &to).then_some(f)
}

/// The erased `FunctionN` class of `f`'s type, a by-name parameter's `Function0`; `None` if not a
/// function type.
fn function_erasure(l: &mut Lower, f: TNodeId) -> Option<Erased> {
    let s = l.tpd.sym(f)?;
    let info = l.u.info(s);
    if l.u.symbol(s).kind == Kind::Param && matches!(l.u.types().get(info), Type::ByName(_)) {
        return Some(Erased::Class("scala.Function0".into()));
    }
    let tpe = l.u.widen(info);
    let class = l.u.class_symbol(tpe)?;
    let name = l.u.full_name(class);
    let function = name.strip_prefix("scala.Function").is_some_and(|n| n.parse::<u8>().is_ok());
    function.then(|| l.u.erase(tpe))
}
