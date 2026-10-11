//! Dropping the bindings of an inline expansion that nothing uses, and inlining the elideable
//! methods used once (dotty's `Inliner.dropUnusedDefs`, without type bindings).

use super::copy::TreeTypeMap;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, map_children};
use rustc_hash::FxHashMap;
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId, Type};

impl Inlining<'_> {
    /// `bindings` without the unused elideable ones and the inline ones, and `expansion` with the
    /// elideable methods used once inlined.
    pub fn drop_unused(
        &mut self,
        bindings: Vec<TNodeId>,
        expansion: TNodeId,
    ) -> Result<(Vec<TNodeId>, TNodeId), Unsupported> {
        let mut count: FxHashMap<SymbolId, u32> = FxHashMap::default();
        let mut rhs_of: FxHashMap<SymbolId, TNodeId> = FxHashMap::default();
        for &b in &bindings {
            let Some(s) = self.tpd.sym(b) else { continue };
            let elideable = match *self.tpd.tree(b) {
                T::DefDef { paramss, rhs: Some(r), .. } if paramss.is_empty() => {
                    self.is_elideable(r).then_some(r)
                }
                T::ValDef { rhs: Some(r), .. } => self.is_elideable(r).then_some(r),
                _ => None,
            };
            if let Some(r) = elideable {
                count.insert(s, 0);
                rhs_of.insert(s, r);
            }
        }
        self.count_refs(expansion, &mut count);
        for &b in &bindings {
            self.count_refs(b, &mut count);
        }
        let retain = |i: &Self, s: SymbolId| {
            let kept = match count.get(&s) {
                Some(&x) => x > 1 || x == 1 && i.u.symbol(s).kind != Kind::Def,
                None => true,
            };
            kept && !i.u.symbol(s).flags.has(INLINE)
        };
        let retained: Vec<TNodeId> =
            bindings.iter().copied().filter(|&b| self.tpd.sym(b).is_none_or(|s| retain(self, s))).collect();
        if retained.len() == bindings.len() {
            return Ok((bindings, expansion));
        }
        let once: FxHashMap<SymbolId, TNodeId> =
            rhs_of.into_iter().filter(|(s, _)| count.get(s) == Some(&1)).collect();
        let expansion = self.inline_once(expansion, &once)?;
        self.drop_unused(retained, expansion)
    }

    /// Adds to `count` the references in `t`: one for each reference tree, two for each term
    /// referred to in a type.
    fn count_refs(&mut self, t: TNodeId, count: &mut FxHashMap<SymbolId, u32>) {
        let refs = matches!(self.tpd.tree(t), T::Ident { .. } | T::Select { .. });
        if refs && let Some(s) = self.tpd.sym(t) {
            count.entry(s).and_modify(|x| *x += 1);
        }
        if (refs || matches!(self.tpd.tree(t), T::New { .. } | T::TypeTree { .. }))
            && let Some(tpe) = self.tpd.get(t).tpe
        {
            self.u.map_type(tpe, &mut |u, part| {
                if let Type::Sym(_, s) = *u.types().get(part)
                    && u.symbol(s).kind.is_term()
                {
                    count.entry(s).and_modify(|x| *x += 2);
                }
                None
            });
        }
        for c in self.tpd.children(t) {
            self.count_refs(c, count);
        }
    }

    /// `t` with each reference to a binding of `once` replaced by a copy of its right-hand side
    /// and blocks without statements by their expressions.
    fn inline_once(
        &mut self,
        t: TNodeId,
        once: &FxHashMap<SymbolId, TNodeId>,
    ) -> Result<TNodeId, Unsupported> {
        let t = match *self.tpd.tree(t) {
            T::Ident { .. } | T::Select { .. } => match self.tpd.sym(t).and_then(|s| once.get(&s)) {
                Some(&rhs) => self.copy(&mut TreeTypeMap::default(), rhs, &mut |_, _, _| None)?,
                None => t,
            },
            T::Block { stats, expr } if stats.is_empty() => expr,
            _ => t,
        };
        let mut failed = None;
        map_children(self, t, &mut |i, c| match i.inline_once(c, once) {
            Ok(n) => n,
            Err(e) => {
                failed.get_or_insert(e);
                c
            }
        });
        failed.map_or(Ok(t), Err)
    }
}
