//! Type trees: named types, applied types, singleton types (dotty's `typedType`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::Type;

impl Typer<'_> {
    pub fn typed_type(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        if let Some(t) = self.ready(id) {
            return t;
        }
        let span = self.ast.span(id);
        match self.ast.tree(id).clone() {
            Tree::Ident { .. } | Tree::Select { .. } => self.typed(id, Pt::Any, ctx),
            Tree::AppliedTypeTree { tpt, args } => {
                let tpt = self.typed_type(tpt, ctx);
                let arg_ids = self.ast.items(args).to_vec();
                let typed_args: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
                let arg_types = typed_args.iter().map(|&a| self.tpd.tpe(a)).collect();
                let tycon = self.tpd.tpe(tpt);
                let tpe = self.u.intern_type(Type::Applied(tycon, arg_types));
                let args = self.tpd.list(&typed_args);
                self.tpd.node(T::AppliedTypeTree { tpt, args }, span, Some(tpe), None)
            }
            Tree::SingletonTypeTree { reference } => {
                let reference = self.typed(reference, Pt::Any, ctx);
                let tpe = self.tpd.tpe(reference);
                self.tpd.node(T::SingletonTypeTree { reference }, span, Some(tpe), None)
            }
            _ => self.unsupported(id, "type tree"),
        }
    }

    /// A type tree of type `tpe` that no source tree gave (dotty's `TypeTree(tpe)`).
    pub fn type_tree(&mut self, tpe: tasty::TypeId, span: syntax::ast::Span, inferred: bool) -> TNodeId {
        self.tpd.node(T::TypeTree { inferred }, span, Some(tpe), None)
    }
}
