//! Assignment operators `x op= y` typed as `x = x op y` when `x` has no member `op=` (dotty's
//! `typedOpAssign`).

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::TypeVars;

/// Whether `name` is an assignment operator such as `+=` (dotty's `isOpAssignmentName`).
fn is_op_assignment_name(name: &str) -> bool {
    let Some(first) = name.chars().next() else { return false };
    !matches!(name, "!=" | "<=" | ">=")
        && name.ends_with('=')
        && first != '='
        && syntax::scan::chars::is_operator_part(first)
}

impl Typer<'_> {
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
        self.ahead.insert(lhs, typed_lhs);
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
            Ok(false) if !pure_prefix => {
                return Some(self.unsupported(id, "assignment operator on a complex prefix"));
            }
            Ok(false) => {}
        }
        let span = self.ast.span(id);
        let lhs_span = self.ast.span(lhs);
        let op = TName { name: self.name(&text[..text.len() - 1]), is_type: false };
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

    /// Whether the type of `qual` has a term member `name`, possibly after an implicit
    /// conversion.
    fn has_member(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> Result<bool, &'static str> {
        let tpe = self.tpd.tpe(qual);
        let tpe = self.widen_if_unstable(tpe);
        let members = self.type_members(tpe, name);
        if members.iter().any(|&s| !self.is_type_symbol(s)) {
            return Ok(true);
        }
        if self.extension_visible(name, ctx) {
            return Err("extension method");
        }
        let mark = self.constraint.mark();
        let converted = self.converted(qual, Goal::Member(name), ctx);
        self.constraint.rollback(mark);
        Ok(converted?.is_some())
    }
}
