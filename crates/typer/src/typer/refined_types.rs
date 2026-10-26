// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Refined type trees (dotty's `typedRefinedTypeTree`), and
//! selections of members only a refinement has.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, TName, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `tpt { type X = … }`: the parent, `Object` if none, refined by each member in turn; a
    /// refinement of a member class refused.
    pub fn typed_refined_type(
        &mut self,
        id: NodeId,
        tpt: Option<NodeId>,
        refinements: List,
        ctx: Ctx,
    ) -> TNodeId {
        let items = self.ast.items(refinements).to_vec();
        let span = self.ast.span(id);
        let parent = match tpt {
            Some(tpt) => {
                let parent = self.typed_type(tpt, ctx);
                if self.is_error(parent) {
                    return parent;
                }
                parent
            }
            None => {
                let at = items.first().map_or(span, |&r| self.ast.span(r)).start_pos();
                let object = self.u.class_ref("java.lang.Object");
                self.type_tree(object, at, false)
            }
        };
        let mut tpe = self.tpd.tpe(parent);
        if self.type_arity(tpe) > 0 {
            return self.unsupported(id, "refinement of a type constructor");
        }
        let class = self.refinement_class(ctx);
        let mut members = Vec::new();
        for &r in &items {
            let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(r) else {
                match self.term_refinement(class, r, ctx) {
                    Ok((name, ..)) if !self.type_members(tpe, &name).is_empty() => {
                        return self.unsupported(r, "structural refinement of an existing member");
                    }
                    Ok((name, info, tree)) => {
                        tpe = self.u.intern_type(Type::Refined(tpe, name, info));
                        members.push(tree);
                    }
                    Err(what) => return self.unsupported(r, what),
                }
                continue;
            };
            if matches!(self.ast.tree(rhs), Tree::Template { .. }) {
                return self.unsupported(r, "higher-kinded refinement");
            }
            if self.refers_to_refinement(rhs, &items, tpe) {
                return self.unsupported(r, "refinement referring to the refined type's members");
            }
            let text = self.text(name.name).to_owned();
            if self.refines_class(tpe, &text) {
                return self.unsupported(r, "refinement of a class member");
            }
            if self.type_members(tpe, &text).is_empty()
                && self.has_variant_params(rhs)
                && !self.is_mirror_type(tpe)
            {
                return self.unsupported(r, "new higher-kinded refinement with variances");
            }
            let (text, info, tree) = match self.type_refinement(class, r, ctx) {
                Ok(refinement) => refinement,
                Err(refused) => return refused,
            };
            tpe = self.u.intern_type(Type::Refined(tpe, text, info));
            members.push(tree);
        }
        let refinements = self.tpd.list(&members);
        self.tpd.node(T::RefinedTypeTree { tpt: parent, refinements }, span, Some(tpe), None)
    }

    /// Whether `rhs` contains `this` or an identifier naming one of `items` or a non-private
    /// member of `parent`.
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
            let members = self.type_members(parent, &text);
            sibling || members.into_iter().any(|m| !self.u.symbol(m).flags.has(tasty::tags::PRIVATE))
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

    /// Whether type lambda tree `rhs` has a covariant or contravariant type parameter.
    fn has_variant_params(&self, rhs: NodeId) -> bool {
        use syntax::ast::flags::{CONTRAVARIANT, COVARIANT};
        let Tree::LambdaTypeTree { tparams, .. } = *self.ast.tree(rhs) else { return false };
        self.ast.items(tparams).iter().any(|&p| {
            self.ast
                .mods_of(p)
                .is_some_and(|m| self.ast.modifiers(m).flags.is_one_of(COVARIANT.or(CONTRAVARIANT)))
        })
    }

    /// Whether `t` has a member class named `name`.
    fn refines_class(&mut self, t: TypeId, name: &str) -> bool {
        let members = self.type_members(t, name);
        members.into_iter().any(|m| self.u.symbol(m).kind == tasty::Kind::Class)
    }
}
