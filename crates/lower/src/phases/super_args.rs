//! Refuses the superclass constructor arguments dotty's `HoistSuperArgs` moves into methods: those
//! defining methods or classes or creating instances of classes that are not static.

use crate::symbols::is_static;
use crate::{Lower, Result, unsupported};
use typer::tpd::{TNodeId, Tree};

pub fn check(l: &mut Lower, td: TNodeId) -> Result<()> {
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
                _ => break,
            }
        }
        for arg in all {
            for t in super::walk::subtrees(l.tpd, arg) {
                let hoisted = match *l.tpd.tree(t) {
                    Tree::DefDef { .. } | Tree::Template { .. } => true,
                    Tree::New { tpt } => {
                        let tpe = l.tpd.tpe(tpt);
                        l.u.class_symbol(tpe).is_some_and(|c| !is_static(l.u, c))
                    }
                    _ => false,
                };
                if hoisted {
                    return unsupported(
                        l.tpd.span(t),
                        "superclass constructor arguments defining or creating local classes or methods",
                    );
                }
            }
        }
    }
    Ok(())
}
