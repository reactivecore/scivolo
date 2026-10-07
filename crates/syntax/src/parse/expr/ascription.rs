//! Assignments and ascriptions after an expression.

use super::super::migration::Migration;
use super::super::{Location, Parser};
use crate::ast::{NodeId, Tree};
use crate::diag::rewrite_notice;
use crate::token::Token;

impl Parser<'_> {
    /// An assignment or ascription after `t`.
    pub fn expr1_rest(&mut self, t: NodeId, location: Location) -> NodeId {
        if self.tok() == Token::Equals {
            let assignable = matches!(
                self.ast.tree(t),
                Tree::Ident { .. }
                    | Tree::Select { .. }
                    | Tree::Apply { .. }
                    | Tree::PrefixOp { .. }
                    | Tree::PostfixOp { .. }
            );
            if !assignable {
                return t;
            }
            let start = self.start_offset(t);
            let point = self.s.skip_token();
            let loc = if location.in_args() { location } else { Location::Elsewhere };
            let rhs = self.sub_part(|p| p.expr_at(loc));
            let a = self.ast.node(Tree::Assign { lhs: t, rhs });
            self.at_span2(start, point, a)
        } else if matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
            self.next();
            self.ascription(t, location)
        } else {
            t
        }
    }

    /// `: Type`, `: @annot...` or `: _*` after `t`.
    pub fn ascription(&mut self, t: NodeId, location: Location) -> NodeId {
        let start = self.start_offset(t);
        let r = match self.tok() {
            Token::Uscore if self.lookahead_is_ident_named("*") => {
                let uscore = self.s.skip_token();
                let vararg = location.in_args() && self.following_is_vararg();
                self.next();
                if vararg {
                    let msg = format!(
                        "The syntax `x: _*` is no longer supported for vararg splices; use `x*` instead{}",
                        rewrite_notice("This construct", "3.4-migration", "")
                    );
                    self.migration_error(msg, uscore, Migration::VarargSpliceAscription);
                } else if !self.op_stack.is_empty() {
                    let msg = "`_*` can be used only for last argument of method application.\nIt is no longer allowed in operands of infix operations.";
                    self.migration_error(msg, uscore, Migration::Scala2to3);
                } else {
                    let msg = crate::diag::Message::syntax(31, "* can be used only for last argument");
                    self.syntax_error(msg, uscore);
                }
                let star = self.type_name("_*");
                let id = self.ast.node(Tree::Ident { name: star });
                let id = self.at_span1(uscore, id);
                self.ast.node(Tree::Typed { expr: t, tpt: id })
            }
            Token::At if !location.in_pattern() => {
                let annots = self.annotations(false);
                annots.into_iter().fold(t, |arg, annot| self.ast.node(Tree::Annotated { arg, annot }))
            }
            _ => {
                let tpt = self.type_depending_on(location);
                if self.is_wildcard(t) && !location.in_pattern() {
                    let vd = *self.placeholder_params.last().expect("a placeholder");
                    let span = self.ast.span(vd).union(self.ast.span(tpt));
                    if let Tree::ValDef { tpt: vt, .. } = &mut self.ast.get_mut(vd).tree {
                        *vt = tpt;
                    }
                    self.ast.get_mut(vd).span = span;
                }
                self.ast.node(Tree::Typed { expr: t, tpt })
            }
        };
        self.at_span1(start, r)
    }

    pub fn lookahead_is_ident_named(&mut self, name: &str) -> bool {
        let la = self.s.lookahead();
        la.token == Token::Identifier && la.name.is_some_and(|n| self.s.names.text(n) == name)
    }
}
