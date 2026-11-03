// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/staging/CrossStageSafety.scala
//! Quotes of a splice, `'{ ${ e } }`, made `e` before `Splicing` makes quotes of its own.

use super::{SPLICE, quoted, staged};
use crate::Lower;
use crate::symbols::has;
use tasty::SymbolId;
use tasty::tags::INLINE;
use typer::tpd::{TNodeId, Tree, Walk, traverse};

/// Makes each quote of a splice under unit `root`, owned by `owner`, outside its inline methods,
/// the splice's content applied to the quote's `Quotes`.
pub fn cancel(l: &mut Lower, root: TNodeId, owner: SymbolId) {
    traverse(l, root, owner, &mut |l, t, _| {
        if matches!(l.tpd.tree(t), Tree::DefDef { .. }) && l.tpd.sym(t).is_some_and(|s| has(l.u, s, INLINE)) {
            return Walk::Skip;
        }
        if let Some(q) = quoted(l, t)
            && let Some(content) = cancelled(l, q.body)
            && let Tree::Apply { fun, .. } = *l.tpd.tree(t)
            && let Tree::Select { qualifier, .. } = *l.tpd.tree(fun)
        {
            l.copy_node(qualifier, content);
        }
        Walk::Children
    });
}

/// The content of the splice quote body `t` is, past blocks without statements (dotty's
/// `CancelledQuote`).
fn cancelled(l: &Lower, t: TNodeId) -> Option<TNodeId> {
    match *l.tpd.tree(t) {
        Tree::Block { stats, expr } if l.tpd.items(stats).is_empty() => cancelled(l, expr),
        _ => staged(l, t, SPLICE).map(|(content, _)| content),
    }
}
