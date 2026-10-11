//! Refuses the superclass constructor arguments dotty's `HoistSuperArgs` moves out of a class
//! nested in a class, into the enclosing class.

use super::hoist_super_args::needs_hoist;
use super::local_free::is_term;
use crate::{Lower, Result, unsupported};
use tasty::Kind;
use typer::tpd::{TNodeId, Tree};

pub fn check(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    if l.u.symbol(class).owner.is_some_and(|o| l.u.symbol(o).kind == Kind::Package || is_term(l, o)) {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return Ok(()) };
    let Tree::Template { parents, .. } = *l.tpd.tree(template) else { return Ok(()) };
    for parent in l.tpd.items(parents).to_vec() {
        let (mut fun, mut all) = (parent, Vec::new());
        loop {
            match *l.tpd.tree(fun) {
                Tree::Apply { fun: inner, args } => {
                    all.extend(l.tpd.items(args).iter().copied());
                    fun = inner;
                }
                Tree::TypeApply { fun: inner, .. } => fun = inner,
                Tree::Block { expr, .. } => fun = expr,
                _ => break,
            }
        }
        if let Some(arg) = all.into_iter().find(|&a| needs_hoist(l, a)) {
            return unsupported(
                l.tpd.span(arg),
                "superclass constructor arguments defining or creating local classes or methods",
            );
        }
    }
    Ok(())
}
