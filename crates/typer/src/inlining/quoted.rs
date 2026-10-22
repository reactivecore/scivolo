// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/PickledQuotes.scala
//! The expression a macro returned, built from its quotes (dotty's `PickledQuotes.spliceTerms`
//! and `spliceTypes` after unpickling): each quote copied with its holes filled and its spliced
//! types replaced.

use super::copy::TreeTypeMap;
use super::macros::{Expanded, Quoted};
use super::{Inlining, Unsupported};
use crate::tpd::TNodeId;
use rustc_hash::FxHashMap;
use syntax::ast::Span;

impl Inlining<'_> {
    /// The tree of the expression the macro returned; `args` are the trees of its `Expr`
    /// arguments, which come from the call's source, as the trees the macro built do. A hole
    /// filled with such a tree gets the hole's span.
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
                let mut from_call = Vec::new();
                for &(h, e) in holes {
                    let filled = self.quoted_tree((expanded, args), built, e, depth + 1)?;
                    if matches!(expanded.exprs.get(e as usize), Some(Quoted::Tree(_))) {
                        from_call.push(filled);
                    }
                    fills.insert(h, filled);
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
                    if args.contains(&filled) || from_call.contains(&filled) {
                        let span = i.tpd.span(hole);
                        i.tpd.get_mut(copy).span = span;
                        i.position_unpositioned(copy, span);
                    }
                    Some(copy)
                })?;
                failed.map_or(Ok(()), Err)?;
                built[id as usize] = Some(tree);
                Ok(tree)
            }
        }
    }

    /// Gives the trees in `t` without a span `span`: the type trees of `TypeTree.of`.
    fn position_unpositioned(&mut self, t: TNodeId, span: Span) {
        for c in self.tpd.children(t) {
            if !self.tpd.span(c).exists() {
                self.tpd.get_mut(c).span = span;
            }
            self.position_unpositioned(c, span);
        }
    }
}
