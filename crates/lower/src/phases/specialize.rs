//! Calls of a function's `apply` with primitive types (dotty's `SpecializeFunctions`): `f(x)`
//! for `f: Int => Int` calls `f.apply$mcII$sp(x)`, which needs no boxing.

use super::adapt::from_start;
use super::adapted::specializable;
use super::walk::try_walk_class;
use crate::definitions::specialized_apply;
use crate::{Lower, Result, unsupported};
use tasty::{Erased, Type};
use typer::tpd::{TNodeId, Tree, Walk};

/// Specializes the calls of functions' `apply` in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    try_walk_class(l, td, &mut |l, t, _| {
        if let Tree::Apply { fun, args } = *l.tpd.tree(t) {
            specialize(l, t, fun, l.tpd.items(args).len())?;
        }
        Ok(Walk::Children)
    })
}

/// Makes call `app` of `fun` with `arity` arguments call the specialized `apply` if `fun` is a
/// function's `apply` of specializable primitive types.
fn specialize(l: &mut Lower, app: TNodeId, fun: TNodeId, arity: usize) -> Result<()> {
    let (Tree::Select { qualifier, .. }, Some(sym)) = (l.tpd.tree(fun), l.tpd.sym(fun)) else {
        return Ok(());
    };
    let qualifier = *qualifier;
    let owner = l.u.symbol(sym).owner.map(|o| l.u.full_name(o));
    if arity > 2 || l.u.symbol(sym).name != "apply" || owner != Some(format!("scala.Function{arity}")) {
        return Ok(());
    }
    let method = l.u.widen(l.tpd.tpe(fun));
    let Type::Method { params, .. } = l.u.types().get(method).clone() else { return Ok(()) };
    let params: Vec<Erased> = params.iter().map(|&(_, t)| l.u.erase(t)).collect();
    let result = l.u.widen(l.tpd.tpe(app));
    let result = l.u.erase(result);
    let primitives = params.iter().all(|p| matches!(p, Erased::Prim(_)));
    if !primitives || !specializable(&params, &result) {
        return Ok(());
    }
    let Ok(specialized) = specialized_apply(l.u, &params, &result) else {
        return unsupported(l.tpd.span(app), "specialized functions");
    };
    let prefix = l.tpd.tpe(qualifier);
    let tpe = l.u.intern_type(Type::Sym(Some(prefix), specialized));
    let node = l.tpd.get_mut(fun);
    (node.sym, node.tpe) = (Some(specialized), Some(tpe));
    let node = l.tpd.get_mut(app);
    node.span = from_start(node.span);
    Ok(())
}
