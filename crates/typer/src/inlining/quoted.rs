// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/PickledQuotes.scala
//! The expression a macro returned, built from its quotes (dotty's `PickledQuotes.spliceTerms`
//! and `spliceTypes` after unpickling): each quote copied with its holes filled and its spliced
//! types replaced, also where a tree the macro built holds it.

use super::copy::TreeTypeMap;
use super::macros::{Expanded, Quoted};
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::{SymbolId, Universe};

/// The expansion, the trees of the macro's arguments, the quotes by their roots and the filling
/// expressions by their hole placeholders.
type Context<'c> = (&'c Expanded, &'c [TNodeId], &'c FxHashMap<TNodeId, u32>, &'c FxHashMap<TNodeId, u32>);

impl Inlining<'_> {
    /// The trees of the expressions the macro returned; `args` are the trees of its `Expr`
    /// arguments, which come from the call's source, as the trees the macro built do. A hole
    /// filled with such a tree gets the hole's span.
    pub fn quoted_trees(
        &mut self,
        expanded: &Expanded,
        args: &[TNodeId],
    ) -> Result<Vec<TNodeId>, Unsupported> {
        let mut built = vec![None; expanded.exprs.len()];
        let roots: FxHashMap<TNodeId, u32> = (expanded.exprs.iter().enumerate())
            .filter_map(|(e, q)| match q {
                Quoted::Quote { root, .. } => Some((*root, e as u32)),
                Quoted::Tree(_) => None,
            })
            .collect();
        let fills: FxHashMap<TNodeId, u32> = (expanded.exprs.iter())
            .flat_map(|q| match q {
                Quoted::Quote { holes, .. } => holes.as_slice(),
                Quoted::Tree(_) => &[],
            })
            .copied()
            .collect();
        let cx = (expanded, args, &roots, &fills);
        let mut trees = Vec::new();
        for &e in &expanded.results {
            let tree = self.quoted_tree(cx, &mut built, e, 0)?;
            trees.push(match expanded.exprs[e as usize] {
                Quoted::Quote { .. } => tree,
                Quoted::Tree(t) => {
                    let kept = self.definition_symbols(t).into_iter().map(|s| (s, s)).collect();
                    let m = TreeTypeMap { syms: kept, ..TreeTypeMap::default() };
                    self.with_quotes(cx, &mut built, (&m, None), t, None)?
                }
            });
        }
        Ok(trees)
    }

    /// A copy of `t` through a map of `m`'s references and symbols, its outermost definitions
    /// owned by `(_, owner)` (dotty's `changeOwner`), the quotes the macro built it of but quote
    /// `own` replaced by their trees.
    fn with_quotes(
        &mut self,
        cx: Context<'_>,
        built: &mut Vec<Option<TNodeId>>,
        (m, owner): (&TreeTypeMap, Option<SymbolId>),
        t: TNodeId,
        own: Option<u32>,
    ) -> Result<TNodeId, Unsupported> {
        let mut inner = TreeTypeMap { refs: m.refs.clone(), syms: m.syms.clone(), ..TreeTypeMap::default() };
        let mut failed = None;
        let copy = self.copy(&mut inner, t, &mut |i, m, node| {
            let (e, own) = match cx.2.get(&node).copied().filter(|&e| Some(e) != own) {
                Some(e) => (e, Some(e)),
                None => (cx.3.get(&node).copied()?, None),
            };
            let quote =
                i.quoted_tree(cx, built, e, 1).and_then(|q| i.with_quotes(cx, built, (m, None), q, own));
            Some(quote.unwrap_or_else(|e| {
                failed.get_or_insert(e);
                node
            }))
        });
        let copy = failed.map_or(copy, Err)?;
        if let Some(owner) = owner {
            self.reown(copy, owner);
        }
        Ok(copy)
    }

    /// The tree of expression `id`: a quote's built once, its holes' first.
    fn quoted_tree(
        &mut self,
        cx: Context<'_>,
        built: &mut Vec<Option<TNodeId>>,
        id: u32,
        depth: usize,
    ) -> Result<TNodeId, Unsupported> {
        let (expanded, args, _, _) = cx;
        let unknown = (self.call_span, "a macro expression the frontend did not give it");
        match expanded.exprs.get(id as usize).ok_or(unknown)? {
            _ if depth > expanded.exprs.len() => Err(unknown),
            &Quoted::Tree(t) => Ok(t),
            Quoted::Quote { .. } if built[id as usize].is_some() => Ok(built[id as usize].expect("built")),
            Quoted::Quote { root, spliced, holes } => {
                let mut fills = FxHashMap::default();
                let mut from_call = Vec::new();
                for &(h, e) in holes {
                    let filled = self.quoted_tree(cx, built, e, depth + 1)?;
                    if matches!(expanded.exprs.get(e as usize), Some(Quoted::Tree(_))) {
                        from_call.push(filled);
                    }
                    fills.insert(h, filled);
                }
                let mut owners = FxHashMap::default();
                self.hole_owners(*root, None, &fills, &mut owners);
                let mut m = TreeTypeMap { refs: spliced.iter().copied().collect(), ..TreeTypeMap::default() };
                let mut failed = None;
                let tree = self.copy(&mut m, *root, &mut |i, m, hole| {
                    let filled = *fills.get(&hole)?;
                    let owner = owners.get(&hole).map(|o| m.syms.get(o).copied().unwrap_or(*o));
                    let copy = i.with_quotes(cx, built, (m, owner), filled, Some(id)).unwrap_or_else(|e| {
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

    /// Records the innermost definition enclosing each hole of `holes` in `t`, `owner` outside.
    fn hole_owners(
        &self,
        t: TNodeId,
        owner: Option<SymbolId>,
        holes: &FxHashMap<TNodeId, TNodeId>,
        out: &mut FxHashMap<TNodeId, SymbolId>,
    ) {
        if let Some(o) = owner.filter(|_| holes.contains_key(&t)) {
            out.insert(t, o);
        }
        let inner = match self.tpd.tree(t) {
            T::DefDef { .. } | T::ValDef { .. } => self.tpd.sym(t).or(owner),
            _ => owner,
        };
        for c in self.tpd.children(t) {
            self.hole_owners(c, inner, holes, out);
        }
    }

    /// Gives the outermost definitions in copy `t` owner `owner`.
    pub fn reown(&mut self, t: TNodeId, owner: SymbolId) {
        reown(self.tpd, self.u, t, owner);
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

/// Gives the outermost definitions in `t` owner `owner`.
pub fn reown(tpd: &Tpd, u: &mut Universe, t: TNodeId, owner: SymbolId) {
    let (mut todo, mut defs) = (vec![t], Vec::new());
    while let Some(n) = todo.pop() {
        if matches!(tpd.tree(n), T::DefDef { .. } | T::ValDef { .. } | T::TypeDef { .. } | T::Bind { .. }) {
            defs.extend(tpd.sym(n));
        }
        todo.extend(tpd.children(n));
    }
    let defined: FxHashSet<SymbolId> = defs.iter().copied().collect();
    for s in defs {
        if u.symbol(s).owner.is_some_and(|o| !defined.contains(&o)) {
            u.set_owner(s, owner);
        }
    }
}
