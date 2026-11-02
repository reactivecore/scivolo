// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Capture sets on function and by-name types, `A ->{c} B` and `->{c} A` (dotty's
//! `typedFunctionType` and `typedByNameTypeTree` of a `CapturesAndResult`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{List, NodeId, Tree};

impl Typer<'_> {
    /// Function type `id` of result `result` with capture set `refs` as
    /// `(args) -> result @retains[refs]`.
    pub fn typed_retaining_function(
        &mut self,
        id: NodeId,
        (refs, result): (List, NodeId),
        ctx: Ctx,
    ) -> TNodeId {
        let Tree::Function { args, mods, wildcard, .. } = self.ast.tree(id).clone() else {
            unreachable!("a function type")
        };
        let function = self.ast.node(Tree::Function { args, body: result, mods, wildcard });
        let span = self.ast.span(id);
        self.ast.with_span(function, span);
        let refs = self.ast.items(refs).to_vec();
        let retaining = self.ast.make_retaining(self.names, function, &refs, "retains");
        self.typed_type(retaining, ctx)
    }

    /// By-name result `result`, `parent @retainsByName[refs]` if it has capture set `refs`.
    pub fn by_name_retaining(&mut self, result: NodeId) -> NodeId {
        let Tree::CapturesAndResult { refs, parent } = *self.ast.tree(result) else { return result };
        let refs = self.ast.items(refs).to_vec();
        self.ast.make_retaining(self.names, parent, &refs, "retainsByName")
    }
}
