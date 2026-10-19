// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Scanners.scala
//! Adjusting a token after reading it, and the parser's hooks into indentation.

use super::Scanner;
use super::chars::is_identifier_start;
use super::region::RegionKind;
use crate::names::Name;
use crate::token::Token;
use crate::token::sets::{ACCEPT_OUTDENT_TOKENS, CLOSING_PARENS, COLON_EOL_PREDECESSORS, END_MARKER_TOKENS};

impl Scanner<'_> {
    /// Fuse `case class`/`case object`, drop `;` before `else` and trailing commas, classify
    /// `:` and `end`, and close indented regions at closing parentheses and the end.
    pub fn post_process_token(&mut self, last_token: Token, last_name: Option<Name>) {
        match self.cur.token {
            Token::Case => {
                self.peek_ahead();
                match self.cur.token {
                    Token::Class => self.fuse(Token::CaseClass),
                    Token::Object => self.fuse(Token::CaseObject),
                    _ => self.reset(),
                }
            }
            Token::Semi => {
                self.peek_ahead();
                if self.cur.token != Token::Else {
                    self.reset();
                }
            }
            Token::Comma => {
                let r = self.region;
                if CLOSING_PARENS.contains(self.regions.closed_by(r)) && self.regions.get(r).commas_expected {
                    self.peek_ahead();
                    if self.cur.is_after_line_end() && CLOSING_PARENS.contains(self.cur.token) {
                        self.cur.last_offset = self.prev.last_offset;
                    } else {
                        self.reset();
                    }
                }
            }
            Token::Arrow => {
                let r = self.region;
                let in_case =
                    self.regions.get(r).outer.is_some_and(|o| self.regions.get(o).kind == RegionKind::InCase);
                if self.regions.is_indented(r) && in_case {
                    self.insert(Token::Outdent, self.cur.offset);
                }
            }
            Token::End => self.cur.token = if self.is_end_marker() { Token::End } else { Token::Identifier },
            Token::ColonOp => {
                let alnum = last_token == Token::Identifier
                    && last_name
                        .and_then(|n| self.names.text(n).chars().next())
                        .is_some_and(|c| (c as u32) <= 0xFFFF && is_identifier_start(c));
                if alnum || COLON_EOL_PREDECESSORS.contains(last_token) {
                    self.cur.token = Token::ColonFollow;
                }
            }
            Token::RBrace | Token::RParen | Token::RBracket => self.close_indented(),
            Token::Eof if !self.source.maybe_incomplete => self.close_indented(),
            _ => {}
        }
    }

    fn fuse(&mut self, token: Token) {
        self.cur.token = token;
        self.cur.offset = self.prev.offset;
        self.cur.last_offset = self.prev.last_offset;
        self.cur.line_offset = self.prev.line_offset;
    }

    /// Whether the current `end` starts an end marker: it begins a line, and a tag token follows
    /// on the same line as the last token there.
    pub fn is_end_marker(&mut self) -> bool {
        if !(self.check_end_markers && self.indent_syntax && self.cur.is_after_line_end()) {
            return false;
        }
        let source = self.source;
        let end_line = source.offset_to_line(self.cur.offset);
        let mut la = self.lookahead_scanner();
        la.check_end_markers = false;
        la.next_token();
        if !(END_MARKER_TOKENS.contains(la.cur.token) && source.offset_to_line(la.cur.offset) == end_line) {
            return false;
        }
        la.next_token();
        la.cur.token == Token::Eof || source.offset_to_line(la.cur.offset) > end_line
    }

    /// Turn a `:` at the end of a line into COLONeol, as the parser asks where a block may start.
    pub fn observe_colon_eol(&mut self, in_template: bool) {
        let enabled = if self.cur.token == Token::ColonOp && in_template {
            self.diags.warning(
                self.cur.offset,
                "`:` after symbolic operator is deprecated; use backticks around operator instead",
            );
            true
        } else {
            self.cur.token == Token::ColonFollow
                && (in_template || self.cfg.source_version.enables_fewer_braces())
        };
        if enabled {
            self.peek_ahead();
            let at_eol = self.cur.is_after_line_end() || self.cur.token == Token::Eof;
            self.reset();
            if at_eol {
                self.cur.token = Token::ColonEol;
            }
        }
    }

    /// After `=>` at the end of a line, start an indented region.
    pub fn observe_arrow_indented(&mut self) {
        if !(matches!(self.cur.token, Token::Arrow | Token::CtxArrow) && self.indent_syntax) {
            return;
        }
        self.peek_ahead();
        let (at_eol, at_eof) = (self.cur.is_after_line_end(), self.cur.token == Token::Eof);
        self.reset();
        if at_eof {
            self.cur.token = Token::Eof;
        } else if at_eol {
            self.indent_to_next(Token::ColonEol);
        }
    }

    /// At a newline before a more indented line, start an indented region.
    pub fn observe_indented(&mut self) {
        if self.indent_syntax && self.cur.is_new_line() {
            self.indent_to_next(Token::ColonEol);
        }
    }

    fn indent_to_next(&mut self, prefix: Token) {
        let next_width = self.indent_width(self.next.offset);
        if self.regions.indent_width(self.region).lt(&next_width) {
            let kind = RegionKind::Indented { width: next_width, prefix };
            self.region = self.regions.push(kind, Some(self.region));
            self.cur.offset = self.next.offset;
            self.cur.token = Token::Indent;
        }
    }

    /// Insert OUTDENT if the current token closes an indented region, or is a comma an outer
    /// region expects.
    pub fn observe_outdented(&mut self) {
        let r = self.region;
        if !self.regions.is_indented(r) || self.regions.is_outermost(r) || self.next.token != Token::Empty {
            return;
        }
        let outer = self.regions.enclosing(r);
        if ACCEPT_OUTDENT_TOKENS.contains(self.cur.token)
            || self.cur.token == Token::Comma && self.regions.commas_expected_in_enclosing(outer)
        {
            self.insert(Token::Outdent, self.cur.offset);
        }
    }

    pub fn close_indented(&mut self) {
        let r = self.region;
        if self.regions.is_indented(r) && !self.regions.is_outermost(r) {
            self.insert(Token::Outdent, self.cur.offset);
        }
    }
}
