// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Optional newlines, `:` at the end of a line, and end markers.

use super::Parser;
use crate::ast::{Attrs, NodeId, Span, Tree, flags};
use crate::diag::rewrite_notice;
use crate::token::Token;
use crate::token::sets::LITERAL_TOKENS;

impl Parser<'_> {
    pub fn new_line_opt(&mut self) {
        if self.tok() == Token::Newline {
            self.next();
        }
    }

    pub fn new_lines_opt(&mut self) {
        if self.s.cur.is_new_line() {
            self.next();
        }
    }

    pub fn new_line_opt_when_followed_by(&mut self, token: Token) {
        if self.tok() == Token::Newline && self.s.next.token == token {
            self.next();
        }
    }

    pub fn new_lines_opt_when_followed_by(&mut self, token: Token) {
        if self.s.cur.is_new_line() && self.s.next.token == token {
            self.next();
        }
    }

    pub fn new_lines_opt_when_followed_by_name(&mut self, name: &str) {
        let next = self.s.next;
        if self.s.cur.is_new_line()
            && next.token == Token::Identifier
            && next.name.is_some_and(|n| self.s.names.text(n) == name)
        {
            self.next();
        }
    }

    pub fn new_line_opt_when_following(&mut self, p: fn(&Self, Token) -> bool) {
        if self.tok() == Token::Newline && p(self, self.s.next.token) {
            self.new_line_opt();
        }
    }

    pub fn accept_indent(&mut self) {
        if self.tok() != Token::Indent {
            let found = if !self.s.diags.has_errors() {
                Token::Eof.show().to_string()
            } else if self.is_ident() {
                format!("{} {}", self.tok().show(), self.s.name_text())
            } else if LITERAL_TOKENS.contains(self.tok()) {
                let v = self.s.str_val(&self.s.cur).unwrap_or(&[]);
                format!("{} {}", self.tok().show(), String::from_utf16_lossy(v))
            } else {
                self.tok().show().to_string()
            };
            self.syntax_error_or_incomplete(
                format!("indented definitions expected, {found} found"),
                self.offset(),
            );
        }
    }

    pub fn colon_at_eol_opt(&mut self) {
        self.possible_colon_offset = self.last_offset();
        self.s.observe_colon_eol(false);
        if self.tok() == Token::ColonEol {
            self.next();
            self.accept_indent();
        }
    }

    pub fn argument_start(&mut self) {
        self.colon_at_eol_opt();
        if self.s.cfg.source_version.migrate_to_3()
            && self.tok() == Token::Newline
            && self.s.next.token == Token::LBrace
        {
            self.next();
            if self.s.indent_width(self.offset()) == self.s.regions.indent_width(self.s.region) {
                let msg = format!(
                    "This opening brace will start a new statement in Scala 3.\nIt needs to be indented to the right to keep being treated as\nan argument to the previous expression.{}",
                    rewrite_notice("This construct", "3.0-migration", "")
                );
                self.migration_error(msg, self.offset(), super::migration::Migration::Scala2to3);
            }
        }
    }

    /// Where a template body may start; true if it is empty because an end marker follows.
    pub fn new_template_start(&mut self) -> bool {
        self.s.observe_colon_eol(true);
        if self.tok() == Token::ColonEol {
            if self.s.lookahead().token == Token::End {
                self.s.cur.token = Token::Newline;
                return true;
            }
            self.next();
            if self.tok() != Token::LBrace {
                self.accept_indent();
            }
        } else {
            self.new_line_opt_when_followed_by(Token::LBrace);
        }
        false
    }

    /// At `end`: check that the end marker matches the last statement and extend its span.
    pub fn check_end_marker(&mut self, stats: &[NodeId]) {
        if self.tok() != Token::End {
            return;
        }
        let start = self.s.skip_token();
        let matched = stats.last().copied().filter(|&last| self.end_marker_matches(last));
        match matched {
            Some(last) => {
                self.ast.add_attrs(last, Attrs::HAS_END_MARKER);
                let end = self.s.last_char_offset();
                let span = self.ast.span(last).with_end(end);
                self.ast.get_mut(last).span = span;
            }
            None => {
                let end = self.s.last_char_offset();
                self.syntax_error_span("misaligned end marker", Span::synthetic(start, end));
            }
        }
        self.s.cur.token = Token::Identifier;
        self.next();
    }

    fn end_marker_matches(&self, stat: NodeId) -> bool {
        let tok = self.tok();
        let ident_is = |name: crate::names::Name| {
            self.is_ident()
                && self.s.cur.name.is_some_and(|n| self.s.names.text(n) == self.s.names.text(name))
        };
        match self.ast.tree(stat) {
            Tree::ValDef { name, mods, .. }
            | Tree::DefDef { name, mods, .. }
            | Tree::TypeDef { name, mods, .. }
            | Tree::ModuleDef { name, mods, .. } => {
                if !self.s.names.text(name.name).is_empty() {
                    if self.s.names.text(name.name) == "<init>" {
                        tok == Token::This
                    } else {
                        ident_is(name.name)
                    }
                } else {
                    self.ast.modifiers(*mods).is(flags::GIVEN) && tok == Token::Given
                }
            }
            Tree::ExtMethods { .. } => tok == Token::Identifier && self.s.name_text() == "extension",
            Tree::PackageDef { pid, .. } => match self.ast.tree(*pid) {
                Tree::Ident { name } | Tree::Select { name, .. } => ident_is(name.name),
                _ => false,
            },
            Tree::PatDef { .. } => tok == Token::Val,
            Tree::If { .. } => tok == Token::If,
            Tree::WhileDo { .. } => tok == Token::While,
            Tree::ParsedTry { .. } => tok == Token::Try,
            Tree::Match { .. } => tok == Token::Match,
            Tree::New { .. } => tok == Token::New,
            Tree::ForYield { .. } | Tree::ForDo { .. } => tok == Token::For,
            _ => false,
        }
    }
}
