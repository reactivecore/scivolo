//! Refined type trees with type-member refinements (dotty's `typedRefinedTypeTree`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{List, NodeId, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `tpt { type X = … }`: the parent refined by each type member in turn; a structural
    /// refinement, a higher-kinded one and one that refines no type member of the parent refused.
    pub fn typed_refined_type(
        &mut self,
        id: NodeId,
        tpt: Option<NodeId>,
        refinements: List,
        ctx: Ctx,
    ) -> TNodeId {
        let Some(tpt) = tpt else { return self.unsupported(id, "refinement of an anonymous parent") };
        let parent = self.typed_type(tpt, ctx);
        if self.is_error(parent) {
            return parent;
        }
        let mut tpe = self.tpd.tpe(parent);
        if self.type_arity(tpe) > 0 {
            return self.unsupported(id, "refinement of a type constructor");
        }
        for r in self.ast.items(refinements).to_vec() {
            let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(r) else {
                return self.unsupported(r, "structural refinement");
            };
            if matches!(self.ast.tree(rhs), Tree::LambdaTypeTree { .. } | Tree::Template { .. }) {
                return self.unsupported(r, "higher-kinded refinement");
            }
            let text = self.text(name.name).to_owned();
            if !self.refines_type_member(tpe, &text) {
                return self.unsupported(r, "refinement that refines no type member");
            }
            let info = self.type_def_info(rhs, ctx);
            if matches!(self.u.types().get(info), Type::Error) {
                return self.unsupported(r, "refinement of an unsupported type");
            }
            if let Type::Bounds(lo, hi) = *self.u.types().get(info)
                && !self.u.is_subtype(lo, hi)
            {
                return self.unsupported(r, "refinement with conflicting bounds");
            }
            tpe = self.u.intern_type(Type::Refined(tpe, text, info));
        }
        let span = self.ast.span(id);
        self.type_tree(tpe, span, false)
    }

    /// Whether `t` has a type member named `name` that is not a class.
    fn refines_type_member(&mut self, t: TypeId, name: &str) -> bool {
        let members = self.type_members(t, name);
        members.into_iter().any(|m| self.u.symbol(m).kind == tasty::Kind::Type)
    }
}
