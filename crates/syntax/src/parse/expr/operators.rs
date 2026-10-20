// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Postfix, infix and prefix expressions.

use super::super::{Location, ParseKind, Parser};
use crate::ast::{NodeId, Tree};
use crate::token::Token;
use crate::token::sets::CAN_START_INFIX_EXPR_TOKENS;

impl Parser<'_> {
    /// `InfixExpr [id]`, and `*` after the last argument.
    pub fn postfix_expr(&mut self, location: Location) -> NodeId {
        let t = self.prefix_expr(location);
        let t = self.postfix_expr_rest(t, location);
        if location.in_args() && self.following_is_vararg() {
            let start = self.s.skip_token();
            let star = self.type_name("_*");
            let id = self.ast.node(Tree::Ident { name: star });
            let id = self.at_span1(start, id);
            self.ast.node(Tree::Typed { expr: t, tpt: id })
        } else {
            t
        }
    }

    pub fn postfix_expr_rest(&mut self, t: NodeId, location: Location) -> NodeId {
        self.infix_ops(
            t,
            |p, tok| p.s.can_start_expr_tokens().contains(tok),
            Self::prefix_expr,
            location,
            ParseKind::Expr,
            move |p| {
                !(location.in_args() && p.following_is_vararg())
                    && p.next_can_follow_operator(CAN_START_INFIX_EXPR_TOKENS)
            },
        )
    }

    /// `[- | + | ~ | !] SimpleExpr`.
    pub fn prefix_expr(&mut self, location: Location) -> NodeId {
        let unary =
            self.tok() == Token::Identifier && matches!(self.s.name_text(), "-" | "+" | "~" | "!") && {
                let la = self.s.lookahead();
                self.s.can_start_expr_tokens().contains(la.token) && la.line_offset.is_none()
            };
        if !unary {
            return self.simple_expr(location);
        }
        if self.is_negated_number() {
            let start = self.s.skip_token();
            let lit = self.literal(start, false, false);
            self.simple_expr_rest(lit, location, true)
        } else {
            let start = self.offset();
            let op = self.term_ident();
            let od = self.simple_expr(location);
            let t = self.ast.node(Tree::PrefixOp { op, od });
            self.at_span1(start, t)
        }
    }
}
