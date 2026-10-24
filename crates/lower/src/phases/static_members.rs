//! Members annotated `@static`, which dotty's `MoveStatics` moves to the companion class and the
//! backend initializes there: refused where they are defined or referred to.

use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use typer::tpd::TNodeId;

const STATIC: &str = "scala.annotation.static";

pub fn refuse(l: &Lower, root: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, root) {
        if l.tpd.sym(t).is_some_and(|s| l.u.is_annotated(s, STATIC)) {
            return unsupported(l.tpd.span(t), "@static members");
        }
    }
    Ok(())
}
