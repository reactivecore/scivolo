//! Assignment operators `x op= y` typed as `x = x op y` when `x` has no member `op=` (dotty's
//! `typedOpAssign`).

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::{Type, TypeId};

/// Whether `name` is an assignment operator such as `+=` (dotty's `isOpAssignmentName`).
fn is_op_assignment_name(name: &str) -> bool {
    let Some(first) = name.chars().next() else { return false };
    !matches!(name, "!=" | "<=" | ">=")
        && name.ends_with('=')
        && first != '='
        && syntax::scan::chars::is_operator_part(first)
}

/// The operator of assignment operator `name`: `+` for `+=`.
fn op_text(name: &str) -> &str {
    &name[..name.len() - 1]
}

impl Typer<'_> {
    /// `lhs = lhs op args`, or `q.update(as, q.apply(as) op args)` for `lhs` `q.apply(as)`, for
    /// application `id` of `lhs.op=`, the parts of `lhs` already lifted (dotty's `typedOpAssign`).
    fn lifted_op_assign(&mut self, id: NodeId, lhs: TNodeId, op: &str, args: List, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        let splice = |t: &mut Self, tree: TNodeId| {
            let node = t.ast.node(Tree::TypeTree);
            t.ast.with_span(node, t.tpd.span(tree));
            t.splices.insert(node, tree);
            node
        };
        let lhs_span = self.tpd.span(lhs);
        let copy = self.copied_path(lhs);
        let qualifier = splice(self, copy);
        let op = TName { name: self.name(op), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier, name: op });
        self.ast.with_span(select, Span::synthetic(lhs_span.start(), lhs_span.end()));
        let last = *self.ast.items(args).last().expect("an argument");
        let rhs = self.ast.node(Tree::Apply { fun: select, args });
        let span = Span::synthetic(span.start(), self.ast.span(last).end());
        self.ast.with_span(rhs, span);
        let T::Apply { fun, args: lhs_args } = *self.tpd.tree(lhs) else {
            let target = splice(self, lhs);
            let assign = self.ast.node(Tree::Assign { lhs: target, rhs });
            self.ast.with_span(assign, span);
            return self.typed_assign(assign, target, rhs, ctx);
        };
        let receiver = match *self.tpd.tree(fun) {
            T::Select { qualifier, name } if self.text(name.name) == "apply" => qualifier,
            _ => return self.unsupported(id, "assignment operator on a call other than apply"),
        };
        let receiver = splice(self, receiver);
        let name = TName { name: self.name("update"), is_type: false };
        let update = self.ast.node(Tree::Select { qualifier: receiver, name });
        let receiver_span = self.ast.span(receiver);
        self.ast.with_span(update, Span::synthetic(receiver_span.start(), receiver_span.end()));
        let mut items: Vec<NodeId> =
            self.tpd.items(lhs_args).to_vec().into_iter().map(|a| splice(self, a)).collect();
        items.push(rhs);
        let update_args = self.ast.list(&items);
        let call = self.ast.node(Tree::Apply { fun: update, args: update_args });
        self.ast.with_span(call, span);
        self.typed_apply(call, update, update_args, Pt::Any, ctx)
    }

    /// `lhs = lhs op args` for application `id` of `fun`, `lhs.op=`, to `args`; `None` if `fun`
    /// is no assignment operator or `lhs` has a member `op=`.
    pub fn op_assign(&mut self, id: NodeId, fun: NodeId, args: List, ctx: Ctx) -> Option<TNodeId> {
        let Tree::Select { qualifier: lhs, name } = *self.ast.tree(fun) else { return None };
        let text = self.text(name.name).to_owned();
        if name.is_type || !is_op_assignment_name(&text) || self.ast.items(args).is_empty() {
            return None;
        }
        let unsupported = self.unsupported.clone();
        let typed_lhs = self.typed(lhs, Pt::Any, ctx);
        self.typed_ahead(lhs, typed_lhs, ctx);
        let clean = self.unsupported == unsupported && !self.diags.has_errors();
        let has_member = if clean { self.has_member(typed_lhs, &text, ctx) } else { Ok(true) };
        let pure_prefix = match *self.tpd.tree(typed_lhs) {
            T::Ident { .. } => true,
            T::Select { qualifier, .. } => self.is_pure_path(qualifier),
            _ => false,
        };
        match has_member {
            Ok(true) => return None,
            Err(what) => return Some(self.unsupported(id, what)),
            Ok(false) => {}
        }
        let mut defs = Vec::new();
        let lifted = match self.lift_assigned(&mut defs, typed_lhs, ctx) {
            Ok(lifted) => lifted,
            Err(what) => return Some(self.unsupported(id, what)),
        };
        if !defs.is_empty() || !pure_prefix {
            let assign = self.lifted_op_assign(id, lifted, op_text(&text), args, ctx);
            return Some(self.wrap_defs(defs, assign));
        }
        let span = self.ast.span(id);
        let lhs_span = self.ast.span(lhs);
        let op = TName { name: self.name(op_text(&text)), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: lhs, name: op });
        self.ast.with_span(select, Span::synthetic(lhs_span.start(), lhs_span.end()));
        let last = *self.ast.items(args).last().expect("an argument");
        let rhs = self.ast.node(Tree::Apply { fun: select, args });
        let span = Span::synthetic(span.start(), self.ast.span(last).end());
        self.ast.with_span(rhs, span);
        let assign = self.ast.node(Tree::Assign { lhs, rhs });
        self.ast.with_span(assign, span);
        Some(self.typed_assign(assign, lhs, rhs, ctx))
    }

    /// Assignment target `lhs` with the prefix of its selection and the arguments of its call
    /// lifted (dotty's `liftAssigned` with `LiftComplex`).
    fn lift_assigned(
        &mut self,
        defs: &mut Vec<TNodeId>,
        lhs: TNodeId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(lhs);
        let (tpe, sym) = (self.tpd.get(lhs).tpe, self.tpd.sym(lhs));
        match *self.tpd.tree(lhs) {
            T::Select { qualifier, name } => {
                let lifted = self.lift(defs, qualifier, "", true, ctx);
                if lifted == qualifier {
                    return Ok(lhs);
                }
                let sym = sym.ok_or("lifting")?;
                let site = self.tpd.tpe(lifted);
                let tpe = self.ref_type(Some(site), sym);
                Ok(self.tpd.node(T::Select { qualifier: lifted, name }, span, Some(tpe), Some(sym)))
            }
            T::Apply { fun, args } => {
                let fun_tpe = self.tpd.tpe(fun);
                let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
                let Type::Method { params, .. } = self.u.types().get(info).clone() else {
                    return Err("lifting");
                };
                let items = self.tpd.items(args).to_vec();
                if items.len() != params.len() {
                    return Err("assignment operator on a call with repeated arguments");
                }
                let fun = match *self.tpd.tree(fun) {
                    T::TypeApply { fun: select, args: targs } => {
                        let select = self.lift_assigned(defs, select, ctx)?;
                        let types: Vec<TypeId> =
                            self.tpd.items(targs).iter().map(|&a| self.tpd.tpe(a)).collect();
                        let tpe = self.instantiated(select, &types)?;
                        self.tpd.node(
                            T::TypeApply { fun: select, args: targs },
                            self.tpd.span(fun),
                            Some(tpe),
                            None,
                        )
                    }
                    T::Select { .. } => self.lift_assigned(defs, fun, ctx)?,
                    _ => return Err("assignment operator on a complex call"),
                };
                let mut arg_defs = Vec::new();
                let lifted = self.lift_args(&mut arg_defs, &params, &items, true, ctx);
                defs.extend(arg_defs.into_iter().map(|(_, d)| d));
                let args = self.tpd.list(&lifted);
                Ok(self.tpd.node(T::Apply { fun, args }, span, tpe, sym))
            }
            _ => Ok(lhs),
        }
    }

    /// Whether the type of `qual` has a term member `name`, possibly after an implicit
    /// conversion.
    fn has_member(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> Result<bool, &'static str> {
        let tpe = self.tpd.tpe(qual);
        let tpe = self.widen_if_unstable(tpe);
        let members = self.type_members(tpe, name);
        if members.iter().any(|&s| !self.u.is_type_symbol(s)) {
            return Ok(true);
        }
        if self.extension_visible(name, ctx) {
            return Err("extension method");
        }
        let before = self.constraint.attempt();
        let converted = self.converted(qual, Goal::Member(name), ctx);
        self.constraint.abandon(before);
        Ok(converted?.is_some())
    }
}
