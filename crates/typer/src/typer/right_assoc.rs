// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Right-associative operator calls `l op: r`, typed as `r.op:(l)`, with `l` lifted unless pure
//! so that it is evaluated first (dotty's `typedInfixOp`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::Type;

impl Typer<'_> {
    /// Whether application `id` is a right-associative infix operation desugared to a call.
    pub fn is_right_assoc_call(&self, id: NodeId) -> bool {
        let Tree::Apply { fun, .. } = *self.ast.tree(id) else { return false };
        let Tree::Select { name, .. } = *self.ast.tree(fun) else { return false };
        let text = self.text(name.name);
        text.ends_with(':') && self.ast.span(fun).is_synthetic()
    }

    /// Call `tree` with its arguments lifted unless pure (dotty's `LiftImpure.liftArgs`), through
    /// the result of a block.
    pub fn lifted_right_assoc(&mut self, tree: TNodeId, ctx: Ctx) -> TNodeId {
        let mut defs = Vec::new();
        let lifted = self.lift_call_args(&mut defs, tree, ctx);
        self.wrap_defs(defs, lifted)
    }

    fn lift_call_args(&mut self, defs: &mut Vec<TNodeId>, tree: TNodeId, ctx: Ctx) -> TNodeId {
        let (span, tpe) = (self.tpd.span(tree), self.tpd.get(tree).tpe);
        match *self.tpd.tree(tree) {
            T::Apply { fun, args } => {
                let fun_tpe = self.tpd.tpe(fun);
                let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
                let Type::Method { params, .. } = self.u.types().get(info).clone() else { return tree };
                let items = self.tpd.items(args).to_vec();
                if items.len() != params.len() {
                    return tree;
                }
                let mut arg_defs = Vec::new();
                let lifted = self.lift_args(&mut arg_defs, &params, &items, false, ctx);
                if arg_defs.is_empty() {
                    return tree;
                }
                defs.extend(arg_defs.into_iter().map(|(_, d)| d));
                let args = self.tpd.list(&lifted);
                self.tpd.node(T::Apply { fun, args }, span, tpe, None)
            }
            T::Block { stats, expr } => {
                let lifted = self.lift_call_args(defs, expr, ctx);
                if lifted == expr {
                    return tree;
                }
                self.tpd.node(T::Block { stats, expr: lifted }, span, tpe, None)
            }
            _ => tree,
        }
    }
}
