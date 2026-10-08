//! `Expr1`: control structures.

use super::super::migration::Migration;
use super::super::{Location, Parser};
use crate::ast::constant::Constant;
use crate::ast::{Attrs, NodeId, Span, Tree};
use crate::diag::{Message, rewrite_notice};
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    pub fn expr1(&mut self, location: Location) -> NodeId {
        match self.tok() {
            Token::If => self.if_expr(self.offset(), false),
            Token::While => {
                let start = self.s.skip_token();
                let cond = self.cond_expr(Token::Do);
                self.new_lines_opt();
                let body = self.sub_expr();
                let t = self.ast.node(Tree::WhileDo { cond, body });
                self.at_span1(start, t)
            }
            Token::Do => self.do_while(),
            Token::Try => self.try_expr(),
            Token::Throw => {
                let start = self.s.skip_token();
                let expr = self.expr();
                let t = self.ast.node(Tree::Throw { expr });
                self.at_span1(start, t)
            }
            Token::Return => {
                let start = self.s.skip_token();
                let expr = if self.is_expr_intro() { Some(self.expr()) } else { None };
                let t = self.ast.node(Tree::Return { expr });
                self.at_span1(start, t)
            }
            Token::For => self.for_expr(),
            _ => {
                if self.is_ident_named("inline")
                    && !self.s.in_modifier_position()
                    && self.s.can_start_expr_tokens().contains(self.s.lookahead().token)
                {
                    let start = self.s.skip_token();
                    if self.tok() == Token::If {
                        return self.if_expr(start, true);
                    }
                    let t = self.postfix_expr(Location::Elsewhere);
                    if let Tree::Match { kind, .. } = &mut self.ast.get_mut(t).tree {
                        *kind = crate::ast::tree::MatchKind::Inline;
                    } else {
                        self.syntax_error("`inline` must be followed by an `if` or a `match`", start);
                    }
                    t
                } else {
                    let t = self.postfix_expr(location);
                    self.expr1_rest(t, location)
                }
            }
        }
    }

    /// `do body while cond`, rejected but parsed.
    fn do_while(&mut self) -> NodeId {
        let msg = format!(
            "`do <body> while <cond>` is no longer supported,\nuse `while <body> ; <cond> do ()` instead.{}",
            rewrite_notice("This construct", "3.0-migration", "")
        );
        self.migration_error(msg, self.offset(), Migration::Scala2to3);
        let start = self.s.skip_token();
        let body = self.expr();
        if self.s.cur.is_stat_sep() {
            self.next();
        }
        self.accept(Token::While);
        let cond = self.expr();
        let stats = self.ast.list(&[body]);
        let block = self.ast.node(Tree::Block { stats, expr: Some(cond) });
        let unit = self.ast.node(Tree::Literal { value: Constant::Unit });
        let t = self.ast.node(Tree::WhileDo { cond: block, body: unit });
        self.at_span1(start, t)
    }

    fn try_expr(&mut self) -> NodeId {
        let try_offset = self.offset();
        let start = self.s.skip_token();
        let body = self.expr();
        let (handler, handler_start) = if self.tok() == Token::Catch {
            let span = self.offset();
            self.next();
            let h = if self.tok() == Token::Case { self.single_case_match() } else { self.sub_expr() };
            (Some(h), span)
        } else {
            (None, 0)
        };
        if let Some(h) = handler
            && matches!(self.ast.tree(h), Tree::Block { stats, expr: None } if stats.is_empty())
        {
            let end = self.end_offset(h);
            let msg = "The catch block does not contain a valid expression, try\nadding a case like - case e: Exception => to the block";
            let msg = Message::syntax(1, msg);
            self.syntax_error_or_incomplete_span(msg, Span::synthetic(handler_start, end));
        }
        let finalizer = if self.tok() == Token::Finally {
            self.next();
            let e = self.sub_expr();
            if self.ast.span(e).exists() {
                Some(e)
            } else {
                let unit = self.ast.node(Tree::Literal { value: Constant::Unit });
                self.ast.add_attrs(unit, Attrs::SYNTHETIC_UNIT);
                Some(unit)
            }
        } else {
            if handler.is_none() {
                let end = self.end_offset(body);
                let msg = "A try without catch or finally is equivalent to putting\nits body in a block; no exceptions are handled.";
                self.s.diags.warning_at(try_offset, end, try_offset, Message::syntax(2, msg));
            }
            None
        };
        let t = self.ast.node(Tree::ParsedTry { expr: body, handler, finalizer });
        self.at_span1(start, t)
    }

    /// `if (cond) {nl} expr [[semi] else expr]` or `if cond then expr [[semi] else expr]`.
    pub fn if_expr(&mut self, start: Offset, inline: bool) -> NodeId {
        let point = self.s.skip_token();
        let cond = self.cond_expr(Token::Then);
        self.new_lines_opt();
        let thenp = self.sub_expr();
        let elsep = if self.tok() == Token::Else {
            self.next();
            Some(self.sub_expr())
        } else {
            None
        };
        let t = self.ast.node(Tree::If { cond, thenp, elsep, inline });
        self.at_span2(start, point, t)
    }

    pub fn is_expr_intro(&mut self) -> bool {
        self.s.can_start_expr_tokens().contains(self.tok())
            && !self.s.is_soft_modifier_in_modifier_position()
            && !(self.is_ident_named("extension") && self.following_is_extension())
    }
}
