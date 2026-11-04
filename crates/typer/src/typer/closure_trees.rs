// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The typed tree of a function value: its anonymous method and a `Closure` of it in a block.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::NodeId;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `{ def $anonfun(params): result = body; closure($anonfun) }` of type `fun_tpe` at the span
    /// of function `id`, the closure given that type if `sam`.
    pub fn closure_tree(
        &mut self,
        id: NodeId,
        anonfun: tasty::SymbolId,
        params: &[TNodeId],
        (result, body): (TypeId, TNodeId),
        fun_tpe: TypeId,
        sam: bool,
    ) -> TNodeId {
        let span = self.ast.span(id);
        let result_at = match params.last() {
            Some(&p) => self.tpd.span(p).end_pos(),
            None => self.tpd.span(body).start_pos(),
        };
        let tpt = self.type_tree(result, result_at, true);
        let def = self.build().def_def(anonfun, &[params], tpt, Some(body), span);
        let meth_tpe = self.u.intern_type(Type::Sym(None, anonfun));
        let meth = self.build().ident(anonfun, meth_tpe, span.start_pos());
        let env = self.tpd.list(&[]);
        let tpt = sam.then(|| self.type_tree(fun_tpe, span.start_pos(), false));
        let tpe = if sam { fun_tpe } else { self.simplified(fun_tpe) };
        let closure = self.tpd.node(T::Closure { env, meth, tpt }, span, Some(tpe), None);
        let stats = self.tpd.list(&[def]);
        self.build().block(stats, closure, span.to_synthetic())
    }
}
