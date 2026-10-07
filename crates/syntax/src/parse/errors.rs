//! Syntax errors, accepting expected tokens, and recovery.

use super::Parser;
use crate::ast::{NodeId, Span, Tree};
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// Report an error at `offset` unless one was reported there or later already.
    pub fn syntax_error(&mut self, msg: impl Into<String>, offset: Offset) {
        if self.last_error_offset.is_some_and(|l| offset <= l) {
            return;
        }
        let length = if offset == self.offset() && self.s.cur.name.is_some() {
            self.s.name_text().encode_utf16().count()
        } else {
            0
        };
        let end = self.s.source.advance_utf16(offset, length);
        self.syntax_error_span(msg, Span::synthetic(offset, end));
        self.last_error_offset = Some(self.offset());
    }

    /// Report an error at `span`, unless the parser gave up on the source.
    pub fn syntax_error_span(&mut self, msg: impl Into<String>, span: Span) {
        if self.gave_up {
            return;
        }
        if span.exists() {
            self.s.diags.error_at(span.start(), span.end(), span.point(), msg);
        } else {
            self.s.diags.error_unpositioned(msg);
        }
    }

    pub fn warning(&mut self, msg: impl Into<String>, offset: Offset) {
        self.s.diags.warning(offset, msg);
    }

    pub fn incomplete_input_error(&mut self, msg: impl Into<String>) {
        if !self.gave_up && Some(self.offset()) != self.last_error_offset {
            let o = self.offset();
            self.s.diags.incomplete_input_error(o, msg);
        }
    }

    /// At EOF an incomplete-input error, else a syntax error and a skip to a safe point.
    pub fn syntax_error_or_incomplete(&mut self, msg: impl Into<String>, offset: Offset) {
        if self.tok() == Token::Eof {
            self.incomplete_input_error(msg);
        } else {
            self.syntax_error(msg, offset);
            self.skip();
        }
    }

    pub fn syntax_error_or_incomplete_span(&mut self, msg: impl Into<String>, span: Span) {
        if self.tok() == Token::Eof {
            self.incomplete_input_error(msg);
        } else {
            self.syntax_error_span(msg, span);
            self.skip();
        }
    }

    pub fn skip(&mut self) {
        self.s.skip();
        self.last_error_offset = Some(self.offset());
    }

    /// dotty's `ExpectedTokenButFound` message.
    pub fn expected_but_found(expected: Token, found: Token, prefix: &str, suffix: &str) -> String {
        let expected = match expected {
            Token::Identifier | Token::BackquotedIdent => "an identifier".to_string(),
            t => t.display(),
        };
        format!("{prefix}{expected} expected, but {} found{suffix}", found.display())
    }

    /// dotty's `ExpectedTokenButFoundSoftKeyword` message.
    pub fn expected_but_found_soft(expected: Token, found: Token, soft: &str, advice: &str) -> String {
        let ident = |t| matches!(t, Token::Identifier | Token::BackquotedIdent);
        let expected_text = if ident(expected) { "an identifier".to_string() } else { expected.display() };
        let what =
            if ident(found) || expected == Token::ColonOp { "an identifier" } else { "the soft keyword" };
        let addendum = if advice.is_empty() { String::new() } else { format!("\n{advice}") };
        format!(
            "{expected_text} expected, but {} found\nThe soft keyword `{soft}` was taken as {what} in this context.{addendum}",
            found.display()
        )
    }

    /// Consume `token`, or report that it is missing; return the offset of the current token.
    pub fn accept(&mut self, token: Token) -> Offset {
        self.accept_help(token, "")
    }

    /// `accept` with `help` appended to the error message.
    pub fn accept_help(&mut self, token: Token, help: &str) -> Offset {
        let offset = self.offset();
        if self.tok() != token {
            let msg = Self::expected_but_found(token, self.tok(), "", help);
            self.syntax_error_or_incomplete(msg, self.offset());
        }
        if self.tok() == token {
            self.next();
        }
        offset
    }

    /// Consume identifier `name`, or report that it is missing.
    pub fn accept_name(&mut self, name: &str) -> Offset {
        let offset = self.offset();
        if !self.is_ident_named(name) {
            self.syntax_error_or_incomplete(format!("`{name}` expected"), self.offset());
        }
        if self.is_ident_named(name) {
            self.next();
        }
        offset
    }

    pub fn accept_colon(&mut self) -> Offset {
        let offset = self.offset();
        if matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
            self.next();
            offset
        } else {
            self.accept(Token::ColonOp)
        }
    }

    pub fn accept_stat_sep(&mut self) {
        if self.s.cur.is_new_line() {
            self.next();
        } else {
            self.accept(Token::Semi);
        }
    }

    /// `scala.Predef.???`, the tree for a missing expression, from `start`.
    pub fn error_term_tree(&mut self, start: Offset) -> NodeId {
        let end = if self.tok() == Token::Outdent { start } else { self.offset() };
        let t = self.unimplemented_expr();
        self.at_span(Span::synthetic(start, end), t)
    }

    /// `_root_.scala.Predef.???`.
    pub fn unimplemented_expr(&mut self) -> NodeId {
        let predef = self.term("Predef");
        let qual = self.scala_dot(predef);
        let name = self.term("???");
        self.ast.node(Tree::Select { qualifier: qual, name })
    }

    /// `_root_.scala.<name>`.
    pub fn scala_dot(&mut self, name: crate::ast::TName) -> NodeId {
        let root = self.term("_root_");
        let root = self.ast.node(Tree::Ident { name: root });
        let scala = self.term("scala");
        let scala = self.ast.node(Tree::Select { qualifier: root, name: scala });
        self.ast.node(Tree::Select { qualifier: scala, name })
    }

    /// Whether the current token is identifier `name` (dotty's `isIdent(name)`).
    pub fn is_ident_named(&self, name: &str) -> bool {
        self.tok() == Token::Identifier && self.s.name_text() == name
    }

    pub fn is_ident(&self) -> bool {
        self.s.cur.is_ident()
    }
}
