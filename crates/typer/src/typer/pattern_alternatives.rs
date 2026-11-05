// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Pattern alternatives `p1 | p2 | ...` (dotty's `typedAlternative`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::NOTHING;

impl Typer<'_> {
    /// Pattern `p1 | p2 | ...` against a value of type `pt`.
    pub fn typed_alternative(&mut self, id: NodeId, trees: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let ids = self.ast.items(trees).to_vec();
        let narrowed = self.gadt_narrowed().len();
        let typed: Vec<TNodeId> = ids.iter().map(|&t| self.typed(t, pt, ctx)).collect();
        if self.gadt_narrowed().len() != narrowed {
            return self.unsupported(id, "GADT bounds from pattern alternatives");
        }
        if typed.iter().any(|&t| self.binds_variable(t)) {
            return self.unsupported(id, "variable in an alternative");
        }
        let tpe = typed.iter().fold(NOTHING, |acc, &t| {
            let t = self.tpd.tpe(t);
            self.u.lub(acc, t)
        });
        let trees = self.tpd.list(&typed);
        let span = self.ast.span(id);
        self.tpd.node(T::Alternative { trees }, span, Some(tpe), None)
    }

    /// Whether pattern `t` or a pattern inside it binds a variable, a wildcard type argument's
    /// type aside.
    fn binds_variable(&self, t: TNodeId) -> bool {
        matches!(self.tpd.tree(t), T::Bind { name, .. } if !name.is_type || !self.tpd.span(t).is_synthetic())
            || self.tpd.children(t).into_iter().any(|c| self.binds_variable(c))
    }
}
