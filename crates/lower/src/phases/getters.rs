//! dotty's `Getters`: each value of a class becomes a parameterless getter method with the
//! value's right-hand side.

use crate::symbols::{has, is_class, is_module};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { return Ok(()) };
    for stat in l.tpd.items(body).to_vec() {
        let Tree::ValDef { name, tpt, rhs } = *l.tpd.tree(stat) else { continue };
        let Some(sym) = l.tpd.sym(stat) else { continue };
        let owner = l.u.symbol(sym).owner;
        if is_module(l.u, sym) || !owner.is_some_and(|o| is_class(l.u, o)) {
            continue;
        }
        if has(l.u, sym, MUTABLE) {
            return unsupported(l.tpd.span(stat), "mutable fields");
        }
        if has(l.u, sym, LAZY) {
            return unsupported(l.tpd.span(stat), "lazy values");
        }
        if rhs.is_none() {
            return unsupported(l.tpd.span(stat), "abstract values and constructor parameter fields");
        }
        let flags = l.u.symbol(sym).flags.with(FIELDaccessor);
        l.u.set_flags(sym, flags);
        let paramss = l.tpd.clauses(&[]);
        l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs };
    }
    Ok(())
}
