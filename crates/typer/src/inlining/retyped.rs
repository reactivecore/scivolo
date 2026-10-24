// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The types of retyped nodes, recomputed from their retyped children (dotty's `InlineTyper`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use tasty::SymbolId;

impl Inlining<'_> {
    /// Node `t`, whose children are retyped, retyped itself.
    pub fn retyped(&mut self, t: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let applied = matches!(self.tpd.tree(t), T::Apply { .. });
        let t = self.beta_reduce(t, owner)?;
        match *self.tpd.tree(t) {
            T::Select { .. } | T::Apply { .. } => {
                self.fold_constants(t);
                Ok(self.const_to_literal(t))
            }
            _ if applied => Ok(self.const_to_literal(t)),
            T::Block { stats, expr } => {
                let locals: Vec<SymbolId> =
                    self.tpd.items(stats).iter().filter_map(|&s| self.tpd.sym(s)).collect();
                let tpe = self.tpd.tpe(expr);
                if !locals.iter().any(|&l| self.mentions(tpe, l)) {
                    self.tpd.get_mut(t).tpe = Some(tpe);
                } else if let Some(avoided) = self.avoid(tpe, &locals) {
                    let span = self.tpd.span(expr);
                    let tpt =
                        self.tpd.node(T::TypeTree { inferred: true }, span.end_pos(), Some(avoided), None);
                    let expr =
                        self.tpd.node(T::Typed { expr, tpt }, span.to_synthetic(), Some(avoided), None);
                    self.tpd.get_mut(t).tree = T::Block { stats, expr };
                    self.tpd.get_mut(t).tpe = Some(avoided);
                }
                Ok(t)
            }
            T::Inlined { expansion, .. } => {
                self.tpd.get_mut(t).tpe = Some(self.tpd.tpe(expansion));
                Ok(t)
            }
            T::Match { cases: parts, .. } | T::Alternative { trees: parts } => {
                let parts = self.tpd.items(parts).to_vec();
                self.tpd.get_mut(t).tpe = Some(crate::tpd::lub_of(self.tpd, self.u, &parts));
                Ok(t)
            }
            T::Try { expr, cases, .. } => {
                let parts = [&[expr][..], self.tpd.items(cases)].concat();
                self.tpd.get_mut(t).tpe = Some(crate::tpd::lub_of(self.tpd, self.u, &parts));
                Ok(t)
            }
            _ => Ok(t),
        }
    }
}
