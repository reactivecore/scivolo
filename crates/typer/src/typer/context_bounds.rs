// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Context bound type trees, `T: C` as the evidence's type `C[T]` (dotty's
//! `typedContextBoundTypeTree`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};

impl Typer<'_> {
    pub fn typed_context_bound(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::ContextBoundTypeTree { tycon, param_name, .. } = *self.ast.tree(id) else {
            unreachable!("a context bound")
        };
        let typed_tycon = self.typed_type(tycon, ctx);
        let tycon_tpe = self.tpd.tpe(typed_tycon);
        if self.type_arity(tycon_tpe) == 0 {
            return self.unsupported(id, "context bound of a type without parameters");
        }
        let span = self.ast.span(id);
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, self.tpd.span(typed_tycon));
        self.splices.insert(splice, typed_tycon);
        let tparam = self.ast.node(Tree::Ident { name: param_name });
        self.ast.with_span(tparam, span.with_end(span.point()));
        let args = self.ast.list(&[tparam]);
        let applied = self.ast.node(Tree::AppliedTypeTree { tpt: splice, args });
        let envelope = self.ast.span(splice).union(self.ast.span(tparam));
        self.ast.with_span(applied, envelope.to_synthetic());
        self.typed_type(applied, ctx)
    }
}
