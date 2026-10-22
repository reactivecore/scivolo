// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Assignments (dotty's `typedAssign`): to variables, through `update` and setters, dynamic.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::Kind;

impl Typer<'_> {
    /// Whether identifier `qual`'s type has a structural refinement `setter`.
    fn structural_setter(&mut self, qual: NodeId, setter: &str, ctx: Ctx) -> bool {
        let q = self.typed(qual, Pt::Any, ctx);
        let pre = self.tpd.tpe(q);
        let pre = self.u.widen(pre);
        self.u.term_refinement_named(pre, setter).is_some()
    }

    /// `q.x_=` for typed assignment target `lhs`, `q.x` of tree `lhs_tree`, if `q` has a member
    /// `x_=` (the setter case of dotty's `typedAssign`).
    fn setter_select(&mut self, lhs: TNodeId, lhs_tree: NodeId) -> Option<NodeId> {
        let T::Select { qualifier, name } = *self.tpd.tree(lhs) else { return None };
        if self.is_error(lhs) {
            return None;
        }
        let setter = format!("{}_=", self.text(name.name));
        let pre = self.tpd.tpe(qualifier);
        self.members_found(pre, &setter, false)?;
        let name = TName { name: self.name(&setter), is_type: false };
        let qualifier = self.spliced(qualifier);
        let select = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(select, self.ast.span(lhs_tree));
        Some(select)
    }

    /// `x = v` for a variable `x`, or a dynamic `updateDynamic` call typed for `pt` (dotty's
    /// `typedAssign`); `q.x = v` with a setter `x_=`, structural or a member, becomes `q.x_=(v)`.
    pub fn typed_assign(&mut self, id: NodeId, (lhs, rhs): (NodeId, NodeId), pt: Pt, ctx: Ctx) -> TNodeId {
        if let Tree::Apply { fun, args } = *self.ast.tree(lhs) {
            let name = TName { name: self.name("update"), is_type: false };
            let select = self.ast.node(Tree::Select { qualifier: fun, name });
            let fun_span = self.ast.span(fun);
            self.ast.with_span(select, Span::synthetic(fun_span.start(), fun_span.end()));
            let mut items = self.ast.items(args).to_vec();
            items.push(rhs);
            let args = self.ast.list(&items);
            let update = self.ast.node(Tree::Apply { fun: select, args });
            let span = Span::synthetic(self.ast.span(lhs).start(), self.ast.span(rhs).end());
            self.ast.with_span(update, span);
            return self.typed_apply(update, select, args, Pt::Any, ctx);
        }
        let lhs_tree = lhs;
        let outer = self.dynamic_lhs.replace(lhs);
        let lhs = self.typed(lhs, Pt::Any, ctx.with_mode(ctx.mode.with(Mode::ASSIGNMENT_LHS)));
        self.dynamic_lhs = outer;
        if self.is_pending_dynamic(lhs) {
            return self.typed_dynamic_assign(lhs, rhs, pt, ctx);
        }
        let var = self.tpd.sym(lhs).filter(|&s| {
            let s = self.u.symbol(s);
            s.kind == Kind::Val && s.flags.has(tasty::tags::MUTABLE)
        });
        let Some(var) = var else {
            if let Tree::Select { qualifier, name } = *self.ast.tree(lhs_tree)
                && matches!(self.ast.tree(qualifier), Tree::Ident { .. })
                && !self.is_error(lhs)
                && self.structural_setter(qualifier, &format!("{}_=", self.text(name.name)), ctx)
            {
                let setter = format!("{}_=", self.text(name.name));
                let name = TName { name: self.name(&setter), is_type: false };
                let select = self.ast.node(Tree::Select { qualifier, name });
                self.ast.with_span(select, self.ast.span(lhs_tree));
                let args = self.ast.list(&[rhs]);
                let call = self.ast.node(Tree::Apply { fun: select, args });
                self.ast.with_span(call, self.ast.span(id));
                return self.typed_apply(call, select, args, Pt::Any, ctx);
            }
            if let Some(setter) = self.setter_select(lhs, lhs_tree) {
                let args = self.ast.list(&[rhs]);
                let call = self.ast.node(Tree::Apply { fun: setter, args });
                let span = Span::synthetic(self.ast.span(lhs_tree).start(), self.ast.span(rhs).end());
                self.ast.with_span(call, span);
                return self.typed_apply(call, setter, args, Pt::Any, ctx);
            }
            return self.unsupported(id, "assignment to a non-variable");
        };
        self.complete(var);
        let info = self.u.info(var);
        let rhs = self.typed(rhs, Pt::Type(info), ctx);
        let unit = self.unit_type();
        let span = self.ast.span(id);
        self.tpd.node(T::Assign { lhs, rhs }, span, Some(unit), None)
    }
}
