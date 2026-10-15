//! Methods whose results are context closures, whose parameters dotty's erasure makes the
//! method's own (`ContextFunctionResults`): refused.

use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Refuses the methods of unit `root` whose right-hand side is a context closure of their
/// context function result type (dotty's `annotateContextResults`), and references to library
/// methods annotated `ContextResultCount`.
pub fn refuse(l: &mut Lower, root: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, root) {
        let sym = l.tpd.sym(t);
        let refused = match *l.tpd.tree(t) {
            Tree::DefDef { tpt, rhs: Some(rhs), .. } => {
                sym.is_some_and(|s| l.u.symbol(s).name != "$anonfun")
                    && is_context_function(l, l.tpd.tpe(tpt))
                    && is_closure_def(l, rhs)
            }
            Tree::Ident { .. } | Tree::Select { .. } => sym.is_some_and(|s| is_annotated(l, s)),
            _ => false,
        };
        if refused {
            return unsupported(l.tpd.span(t), "methods with context function results");
        }
    }
    Ok(())
}

/// Whether library method `s` is annotated `ContextResultCount`.
fn is_annotated(l: &Lower, s: SymbolId) -> bool {
    l.u.symbol(s).tree.is_some() && l.u.is_annotated(s, "ContextResultCount")
}

/// Whether `t` is a `ContextFunctionN` type.
fn is_context_function(l: &mut Lower, t: tasty::TypeId) -> bool {
    let t = l.u.dealias(t);
    let Type::Applied(tycon, _) = *l.u.types().get(t) else { return false };
    l.u.class_symbol(tycon).is_some_and(|c| l.u.full_name(c).starts_with("scala.ContextFunction"))
}

/// Whether `t` is a closure of the method it defines, in blocks without statements (dotty's
/// `closureDef`).
fn is_closure_def(l: &Lower, t: TNodeId) -> bool {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return false };
    match l.tpd.items(stats) {
        [] => is_closure_def(l, expr),
        &[def] => {
            matches!(*l.tpd.tree(expr), Tree::Closure { meth, .. } if l.tpd.sym(meth) == l.tpd.sym(def))
        }
        _ => false,
    }
}
