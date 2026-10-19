//! The expression a macro returned, built from its quotes (dotty's `PickledQuotes.spliceTerms`
//! and `spliceTypes` after unpickling): each quote copied with its holes filled and its spliced
//! types replaced.

use super::copy::TreeTypeMap;
use super::macros::{Expanded, Quoted};
use super::{Inlining, Unsupported};
use crate::tpd::TNodeId;
use rustc_hash::FxHashMap;

impl Inlining<'_> {
    /// The tree of the expression the macro returned; `args` are the trees of its `Expr`
    /// arguments, which come from the call's source.
    pub fn quoted_trees(&mut self, expanded: &Expanded, args: &[TNodeId]) -> Result<TNodeId, Unsupported> {
        let mut built = vec![None; expanded.exprs.len()];
        let result = self.quoted_tree((expanded, args), &mut built, expanded.result, 0)?;
        match expanded.exprs[expanded.result as usize] {
            Quoted::Quote { .. } => Ok(result),
            Quoted::Tree(t) => self.copy(&mut TreeTypeMap::default(), t, &mut |_, _, _| None),
        }
    }

    /// The tree of expression `id`: a quote's built once, its holes' first.
    fn quoted_tree(
        &mut self,
        (expanded, args): (&Expanded, &[TNodeId]),
        built: &mut Vec<Option<TNodeId>>,
        id: u32,
        depth: usize,
    ) -> Result<TNodeId, Unsupported> {
        let unknown = (self.call_span, "a macro expression the frontend did not give it");
        match expanded.exprs.get(id as usize).ok_or(unknown)? {
            _ if depth > expanded.exprs.len() => Err(unknown),
            &Quoted::Tree(t) => Ok(t),
            Quoted::Quote { .. } if built[id as usize].is_some() => Ok(built[id as usize].expect("built")),
            Quoted::Quote { root, spliced, holes } => {
                let mut fills = FxHashMap::default();
                for &(h, e) in holes {
                    fills.insert(h, self.quoted_tree((expanded, args), built, e, depth + 1)?);
                }
                let mut m = TreeTypeMap { refs: spliced.iter().copied().collect(), ..TreeTypeMap::default() };
                let mut failed = None;
                let tree = self.copy(&mut m, *root, &mut |i, m, hole| {
                    let filled = *fills.get(&hole)?;
                    let mut inner =
                        TreeTypeMap { refs: m.refs.clone(), syms: m.syms.clone(), ..TreeTypeMap::default() };
                    let copy = i.copy(&mut inner, filled, &mut |_, _, _| None).unwrap_or_else(|e| {
                        failed.get_or_insert(e);
                        filled
                    });
                    if args.contains(&filled) {
                        i.tpd.get_mut(copy).span = i.tpd.span(hole);
                    }
                    Some(copy)
                })?;
                failed.map_or(Ok(()), Err)?;
                built[id as usize] = Some(tree);
                Ok(tree)
            }
        }
    }
}
