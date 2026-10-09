//! Values of `scala.Enumeration` defined by `Value` or `Value(i)`: refused (Scala Native's
//! `PrepNativeInterop` names them, #62).

use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use syntax::ast::constant::Constant;
use tasty::{Erased, Kind};
use typer::tpd::{TNodeId, Tree};

/// Fails if a value of class definition `td` is defined by `Value` without a name.
pub fn check(l: &mut Lower, td: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, td) {
        let Tree::ValDef { rhs: Some(rhs), .. } = *l.tpd.tree(t) else { continue };
        if unnamed_value(l, rhs) {
            return unsupported(l.tpd.span(rhs), "Enumeration values without names");
        }
    }
    Ok(())
}

/// Whether `t` is `Value`, `Value(i)`, `Value(null)` or `Value(i, null)` of `Enumeration`.
fn unnamed_value(l: &mut Lower, t: TNodeId) -> bool {
    let (fun, args) = match *l.tpd.tree(t) {
        Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
        _ => (t, Vec::new()),
    };
    let Some(m) = l.tpd.sym(fun) else { return false };
    let s = l.u.symbol(m);
    if s.kind != Kind::Def
        || s.name != "Value"
        || s.owner.is_none_or(|o| l.u.full_name(o) != "scala.Enumeration")
    {
        return false;
    }
    args.into_iter().all(|a| {
        let tpe = l.tpd.tpe(a);
        matches!(l.tpd.tree(a), Tree::Literal { value: Constant::Null })
            || l.u.erase(tpe) == Erased::Prim('i')
    })
}
