// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Capture sets on function and by-name types, `A ->{c} B` and `->{c} A` (dotty's
//! `typedFunctionType` and `typedByNameTypeTree` of a `CapturesAndResult`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::{List, NodeId, Tree};
use tasty::Type;

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
        let retaining = self.ast.make_retaining(self.names, function, &refs, nme::RETAINS);
        self.typed_type(retaining, ctx)
    }

    /// By-name type `id` of result `result`.
    pub fn typed_by_name_type(&mut self, id: NodeId, result: NodeId, ctx: Ctx) -> TNodeId {
        let result = self.by_name_retaining(result);
        let result = self.typed_type(result, ctx);
        let tpe = self.u.intern_type(Type::ByName(self.tpd.tpe(result)));
        self.tpd.node(T::ByNameTypeTree { result }, self.ast.span(id), Some(tpe), None)
    }

    /// By-name result `result`, `parent @retainsByName[refs]` if it has capture set `refs`.
    fn by_name_retaining(&mut self, result: NodeId) -> NodeId {
        let Tree::CapturesAndResult { refs, parent } = *self.ast.tree(result) else { return result };
        let refs = self.ast.items(refs).to_vec();
        self.ast.make_retaining(self.names, parent, &refs, nme::RETAINS_BY_NAME)
    }
}
