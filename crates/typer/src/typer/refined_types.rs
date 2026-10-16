//! Refined type trees with type-member refinements (dotty's `typedRefinedTypeTree`), and
//! selections of members only a refinement has.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, TName, Tree};
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
        let items = self.ast.items(refinements).to_vec();
        for &r in &items {
            let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(r) else {
                return self.unsupported(r, "structural refinement");
            };
            if matches!(self.ast.tree(rhs), Tree::LambdaTypeTree { .. } | Tree::Template { .. }) {
                return self.unsupported(r, "higher-kinded refinement");
            }
            if self.refers_to_refinement(rhs, &items, tpe) {
                return self.unsupported(r, "refinement referring to the refined type's members");
            }
            let text = self.text(name.name).to_owned();
            if !self.refines_type_member(tpe, &text) && !self.is_mirror_type(tpe) {
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

    /// Whether `rhs` contains `this` or an identifier naming one of `items` or a member of
    /// `parent`.
    fn refers_to_refinement(&mut self, rhs: NodeId, items: &[NodeId], parent: TypeId) -> bool {
        if self.ast.exists_subtree(rhs, &|ast, n| matches!(ast.tree(n), Tree::This { .. })) {
            return true;
        }
        let idents = self.ast.subtrees(rhs, &|ast, n| matches!(ast.tree(n), Tree::Ident { .. }));
        idents.into_iter().any(|n| {
            let Tree::Ident { name } = *self.ast.tree(n) else { return false };
            let text = self.text(name.name).to_owned();
            let sibling = items.iter().any(
                |&i| matches!(*self.ast.tree(i), Tree::TypeDef { name: d, .. } if self.text(d.name) == text),
            );
            sibling || !self.type_members(parent, &text).is_empty()
        })
    }

    /// `qual.name` for a type member only a refinement of `qual`'s path type `pre` has (one of
    /// a library's refined types: `MirroredElemTypes` of a `Mirror.Of[T]`); `None` otherwise.
    pub fn refined_select(&mut self, id: NodeId, qual: TNodeId, pre: TypeId, name: TName) -> Option<TNodeId> {
        let stable = match self.u.types().get(pre).clone() {
            Type::Sym(_, s) => !self.u.is_type_symbol(s),
            Type::This(_) | Type::TermRef(..) => true,
            _ => false,
        };
        let text = self.text(name.name).to_owned();
        if !stable || self.u.refinement_named(pre, &text).is_none() {
            return None;
        }
        let tpe = self.u.intern_type(Type::TypeRef(pre, text));
        let span = self.ast.span(id);
        Some(self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), None))
    }

    /// Whether `t` is a refinement of `Mirror` or one of its subtraits, whose type members
    /// `MirroredType` and `MirroredElemTypes` exist only as refinements.
    fn is_mirror_type(&mut self, mut t: TypeId) -> bool {
        while let Type::Refined(parent, ..) = *self.u.types().get(t) {
            t = parent;
        }
        self.u.class_symbol(t).is_some_and(|c| self.u.full_name(c).starts_with("scala.deriving.Mirror"))
    }

    /// Whether `t` has a type member named `name` that is not a class.
    fn refines_type_member(&mut self, t: TypeId, name: &str) -> bool {
        let members = self.type_members(t, name);
        members.into_iter().any(|m| self.u.symbol(m).kind == tasty::Kind::Type)
    }
}
