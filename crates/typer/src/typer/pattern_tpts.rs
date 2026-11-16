//! The type trees a pattern definition's variables share with its typed patterns.

use super::Typer;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::NodeId;

impl Typer<'_> {
    /// Marks the parts of typed type tree `typed` of untyped `tpt` as copies of those of the typed
    /// pattern's type tree that `tpt` is shared with, as dotty's typer shares them: its leaves and
    /// the annotation trees of its annotated type trees.
    pub fn share_pattern_tpt(&mut self, tpt: NodeId, typed: TNodeId) {
        let first = self.pattern_tpts.get(&tpt).copied();
        let Some(pairs) = first.and_then(|of| same_trees(&self.tpd, typed, of)) else { return };
        let annots: Vec<TNodeId> = (pairs.iter())
            .filter_map(|&(c, _)| match *self.tpd.tree(c) {
                T::Annotated { annot, .. } => Some(annot),
                _ => None,
            })
            .collect();
        for (copy, of) in pairs {
            if self.tpd.children(copy).is_empty() || annots.contains(&copy) {
                self.tpd.mark_copy(copy, of);
            }
        }
    }
}

/// The pairs of typed tree `copy` and its descendants with `of`'s, if they have the same kinds,
/// types and spans.
fn same_trees(tpd: &Tpd, copy: TNodeId, of: TNodeId) -> Option<Vec<(TNodeId, TNodeId)>> {
    let (a, b) = (tpd.get(copy), tpd.get(of));
    let same = std::mem::discriminant(&a.tree) == std::mem::discriminant(&b.tree)
        && (a.tpe, a.sym, a.span) == (b.tpe, b.sym, b.span);
    let (copies, ofs) = (tpd.children(copy), tpd.children(of));
    if copy == of || !same || copies.len() != ofs.len() {
        return None;
    }
    let mut pairs = vec![(copy, of)];
    for (c, o) in copies.into_iter().zip(ofs) {
        pairs.extend(same_trees(tpd, c, o)?);
    }
    Some(pairs)
}
