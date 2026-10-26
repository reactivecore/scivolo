// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Throws types `A throws E1 | E2` under `saferExceptions`: `$throws[$throws[A, E1], E2]`
//! (dotty's `typedInfixOp`, `desugar.throws`).

use super::Typer;
use crate::ctx::Ctx;
use syntax::ast::{Attrs, NodeId, Tree};
use syntax::config::Features;

impl Typer<'_> {
    /// Infix type `id`, `left op right`, as `scala.runtime.$throws[left, right]` if `op` is
    /// `throws` and `saferExceptions` is enabled, a union `right` taken apart; `None` otherwise.
    pub fn throws_type(&mut self, id: NodeId, op: NodeId, args: &[NodeId], ctx: Ctx) -> Option<NodeId> {
        let &[left, right] = args else { return None };
        let Tree::Ident { name } = *self.ast.tree(op) else { return None };
        if !self.ast.attrs(id).contains(Attrs::INFIX_OP)
            || !name.is_type
            || self.text(name.name) != "throws"
            || !self.feature_enabled(Features::SAFER_EXCEPTIONS, "experimental.saferExceptions", ctx)
        {
            return None;
        }
        let alias = self.resolve_full_name("scala.runtime.$throws", true)?;
        let alias = crate::defn::type_ref(self.u, alias);
        Some(self.throws_of(left, (op, alias), right))
    }

    fn throws_of(&mut self, left: NodeId, (op, alias): (NodeId, tasty::TypeId), right: NodeId) -> NodeId {
        if let Tree::AppliedTypeTree { tpt: bar, args } = *self.ast.tree(right)
            && self.ast.attrs(right).contains(Attrs::INFIX_OP)
            && matches!(*self.ast.tree(bar), Tree::Ident { name } if self.text(name.name) == "|")
            && let &[l, r] = self.ast.items(args)
        {
            let inner = self.throws_of(left, (op, alias), l);
            return self.throws_of(inner, (bar, alias), r);
        }
        let span = self.ast.span(op).start_pos();
        let typed = self.type_tree(alias, span, false);
        let tycon = self.ast.node(Tree::TypeTree);
        self.ast.with_span(tycon, span);
        self.splices.insert(tycon, typed);
        let args = self.ast.list(&[left, right]);
        let applied = self.ast.node(Tree::AppliedTypeTree { tpt: tycon, args });
        let whole = self.ast.span(left).union(self.ast.span(right));
        self.ast.with_span(applied, whole.to_synthetic());
        applied
    }
}
