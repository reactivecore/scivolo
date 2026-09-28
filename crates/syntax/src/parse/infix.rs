//! Infix operations: the operand/operator stack.

use super::migration::Migration;
use super::precedence::{is_right_assoc, precedence};
use super::{Location, ParseKind, Parser};
use crate::ast::{NodeId, Tree};
use crate::names::Name;
use crate::source::Offset;
use crate::token::Token;

pub struct OpInfo {
    operand: NodeId,
    operator: NodeId,
    offset: Offset,
}

impl Parser<'_> {
    fn op_name(&self, op: NodeId) -> Name {
        match self.ast.tree(op) {
            Tree::Ident { name } => name.name,
            _ => unreachable!("an operator is an identifier"),
        }
    }

    fn check_assoc(&mut self, offset: Offset, op1: Name, op2: Option<Name>, op2_left_assoc: bool) {
        let op1 = self.s.names.text(op1).to_string();
        if is_right_assoc(&op1) == op2_left_assoc {
            let op2 = op2.map_or("null".to_string(), |n| self.s.names.text(n).to_string());
            let (a1, a2) = if op2_left_assoc {
                ("which is right-associative", "which is left-associative")
            } else {
                ("which is left-associative", "which is right-associative")
            };
            let msg = format!("{op1} ({a1}) and {op2} ({a2}) have same precedence and may not be mixed");
            self.syntax_error(msg, offset);
        }
    }

    /// Reduce the operators above `base` that bind tighter than an operator of precedence
    /// `prec`, building infix operations with `top` as right operand.
    fn reduce_stack(
        &mut self,
        base: usize,
        mut top: NodeId,
        prec: i32,
        left_assoc: bool,
        op2: Option<Name>,
        is_type: bool,
    ) -> NodeId {
        if self.op_stack.len() != base {
            let info = self.op_stack.last().expect("an operator");
            let (offset, name) = (info.offset, self.op_name(info.operator));
            if precedence(self.s.names.text(name)) == prec {
                self.check_assoc(offset, name, op2, left_assoc);
            }
        }
        while self.op_stack.len() != base {
            let info = self.op_stack.last().expect("an operator");
            let op_prec = precedence(self.s.names.text(self.op_name(info.operator)));
            if !(prec < op_prec || left_assoc && prec == op_prec) {
                break;
            }
            let info = self.op_stack.pop().expect("an operator");
            let span =
                self.ast.span(info.operator).union(self.ast.span(info.operand)).union(self.ast.span(top));
            let t = self.ast.node(Tree::InfixOp { left: info.operand, op: info.operator, right: top });
            let t = self.at_span(span, t);
            top = self.migrate_infix_op(&info, is_type, t);
        }
        top
    }

    /// Infix operations with named arguments on the right, which read as a named tuple now.
    fn migrate_infix_op(&mut self, info: &OpInfo, is_type: bool, t: NodeId) -> NodeId {
        if is_type {
            return t;
        }
        let op = self.s.names.text(self.op_name(info.operator));
        let named_tuple_op = matches!(op, "==" | "!=" | "eq" | "ne" | "++" | "zip" | "->");
        let Tree::InfixOp { right, .. } = *self.ast.tree(t) else { return t };
        let named_args = match self.ast.tree(right) {
            Tree::Tuple { trees } => {
                self.ast.items(*trees).iter().any(|&a| matches!(self.ast.tree(a), Tree::NamedArg { .. }))
            }
            Tree::Parens { t: inner } => matches!(self.ast.tree(*inner), Tree::Assign { .. }),
            _ => false,
        };
        if named_args && !named_tuple_op {
            let msg = format!(
                "Deprecated syntax: infix named arguments lists are deprecated; since 3.7 it is interpreted as a single named tuple argument.\nTo avoid this warning, either remove the argument names or use dotted selection.{}",
                Self::rewrite_notice("This", "3.7-migration", "")
            );
            let span = self.ast.span(right);
            self.error_or_migration_warning(msg, span, Migration::AmbiguousNamedTupleSyntax);
        }
        t
    }

    /// `operand {infixop operand | MatchClause} [postfixop]` with precedence and associativity.
    pub fn infix_ops(
        &mut self,
        first: NodeId,
        can_start_operand: fn(&Self, Token) -> bool,
        operand: fn(&mut Self, Location) -> NodeId,
        location: Location,
        kind: ParseKind,
        is_operator: impl Fn(&mut Self) -> bool,
    ) -> NodeId {
        let base = self.op_stack.len();
        let is_type = kind == ParseKind::Type;
        let mut top = first;
        let res = loop {
            if self.is_ident() && is_operator(self) {
                let op = if is_type { self.type_ident() } else { self.term_ident() };
                let name = self.op_name(op);
                let text = self.s.names.text(name).to_string();
                let top1 = self.reduce_stack(
                    base,
                    top,
                    precedence(&text),
                    !is_right_assoc(&text),
                    Some(name),
                    is_type,
                );
                let offset = self.offset();
                self.op_stack.push(OpInfo { operand: top1, operator: op, offset });
                self.colon_at_eol_opt();
                self.new_line_opt_when_following(can_start_operand);
                if let Some(lambda) = self.detect_colon_lambda() {
                    self.next();
                    top = self.colon_lambda(lambda);
                    continue;
                }
                let postfix = kind == ParseKind::Expr
                    && self.s.cfg.features.contains(crate::config::Features::POSTFIX_OPS);
                if postfix && !can_start_operand(self, self.tok()) {
                    let info = self.op_stack.pop().expect("an operator");
                    let od = self.reduce_stack(base, info.operand, 0, true, self.s.cur.name, is_type);
                    let start = self.start_offset(od);
                    let t = self.ast.node(Tree::PostfixOp { od, op: info.operator });
                    break self.at_span2(start, info.offset, t);
                }
                top = operand(self, location);
            } else {
                let t = self.reduce_stack(base, top, 0, true, self.s.cur.name, is_type);
                if !is_type && self.tok() == Token::Match {
                    top = self.match_clause(t);
                    let min_infix =
                        self.is_ident() && is_operator(self) && precedence(self.s.name_text()) == 1;
                    if min_infix || self.tok() == Token::Match {
                        continue;
                    }
                    break top;
                }
                break t;
            }
        };
        if self.is_ident_named("*") && !self.following_is_vararg() {
            let o = self.offset();
            self.syntax_error("spread operator `*` not allowed here; must come last in a parameter list", o);
            self.next();
        }
        res
    }
}
