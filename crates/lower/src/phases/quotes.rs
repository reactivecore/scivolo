//! Refuses the quotes outside the splices of inline methods.

use super::walk::children;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use tasty::tags::INLINE;
use typer::tpd::{TNodeId, Tree};

const QUOTED: [&str; 2] = ["scala.quoted.runtime.Expr$.quote", "scala.quoted.Type$.of"];

/// Refuses a term quote or a `Type.of` in unit `root` outside its inline methods.
pub fn refuse(l: &Lower, root: TNodeId) -> Result<()> {
    let mut todo = vec![root];
    while let Some(t) = todo.pop() {
        let sym = l.tpd.sym(t);
        match l.tpd.tree(t) {
            Tree::DefDef { .. } if sym.is_some_and(|s| has(l.u, s, INLINE)) => continue,
            Tree::Ident { .. } | Tree::Select { .. }
                if sym.is_some_and(|s| QUOTED.contains(&l.u.full_name(s).as_str())) =>
            {
                return unsupported(l.tpd.span(t), "quotes outside the splices of inline methods");
            }
            _ => todo.extend(children(l.tpd, t)),
        }
    }
    Ok(())
}
