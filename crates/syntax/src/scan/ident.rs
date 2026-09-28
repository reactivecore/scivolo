//! Identifiers, operators, keywords and soft modifiers.

use super::chars::{is_identifier_part, is_special, is_unicode_identifier_part};
use super::reader::SU;
use super::{Scanner, TokenData};
use crate::config::Features;
use crate::token::sets::{MODIFIER_FOLLOWERS, SCALA3_KEYWORDS, SOFT_MODIFIER_NAMES};
use crate::token::{Token, keyword};

/// Which token data a name is finished into.
#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    Current,
    Next,
}

impl Scanner<'_> {
    pub fn get_backquoted_ident(&mut self) {
        self.reader.next_char();
        self.get_lit_chars('`');
        if self.reader.ch == '`' {
            self.reader.next_char();
            self.finish_named_token(Token::BackquotedIdent, Target::Current);
            match self.name_text() {
                "" => self.error("empty quoted identifier", self.cur.offset),
                "_" => self.error("wildcard invalid as backquoted identifier", self.cur.offset),
                _ => {}
            }
        } else {
            self.error("unclosed quoted identifier", self.cur.offset);
        }
    }

    pub fn get_ident_rest(&mut self) {
        loop {
            let ch = self.reader.ch;
            if ch.is_ascii_alphanumeric() || ch == '$' {
                self.put_char(ch);
                self.reader.next_char();
            } else if ch == '_' {
                self.put_char(ch);
                self.reader.next_char();
                return self.get_ident_or_operator_rest();
            } else if ch != SU && is_unicode_identifier_part(ch) {
                self.put_char(ch);
                self.reader.next_char();
            } else {
                return self.finish_named_token(Token::Identifier, Target::Current);
            }
        }
    }

    pub fn get_operator_rest(&mut self) {
        loop {
            let ch = self.reader.ch;
            let op = match ch {
                '~' | '!' | '@' | '#' | '%' | '^' | '*' | '+' | '-' | '<' | '>' | '?' | ':' | '=' | '&'
                | '|' | '\\' => true,
                '/' => !matches!(self.reader.lookahead_char(), '/' | '*'),
                _ => is_special(ch),
            };
            if !op {
                return self.finish_named_token(Token::Identifier, Target::Current);
            }
            self.put_char(ch);
            self.reader.next_char();
        }
    }

    fn get_ident_or_operator_rest(&mut self) {
        if is_identifier_part(self.reader.ch) { self.get_ident_rest() } else { self.get_operator_rest() }
    }

    /// Set `target`'s name from the buffer and its token; identifiers become keywords, and
    /// `end` only in the current token.
    pub fn finish_named_token(&mut self, token: Token, target: Target) {
        let text = self.lit.take_name();
        let name = self.names.intern(&text);
        let converted = if token == Token::Identifier { self.keyword_token(&text) } else { token };
        let td = match target {
            Target::Current => &mut self.cur,
            Target::Next => &mut self.next,
        };
        td.name = Some(name);
        td.token = token;
        if converted != Token::End || target == Target::Current {
            td.token = converted;
        }
    }

    /// The keyword spelled `text`, or IDENTIFIER (dotty's `toToken`).
    fn keyword_token(&mut self, text: &str) -> Token {
        let Some(kw) = keyword(text) else { return Token::Identifier };
        if SCALA3_KEYWORDS.contains(kw) && self.cfg.source_version.migrate_to_3() {
            self.diags.warning(
                self.cur.offset,
                format!(
                    "{text} is now a keyword, write `{text}` instead of {text} to keep it as an identifier"
                ),
            );
            return Token::Identifier;
        }
        kw
    }

    /// Whether `td` is an operator: backquoted, or ending in an operator character (a UTF-16
    /// unit, as dotty tests it, so never the low half of a supplementary character).
    pub fn is_operator(&self, td: &TokenData) -> bool {
        td.token == Token::BackquotedIdent
            || td.token == Token::Identifier
                && td
                    .name
                    .and_then(|n| self.names.text(n).chars().last())
                    .is_some_and(|c| (c as u32) <= 0xFFFF && super::chars::is_operator_part(c))
    }

    pub fn is_soft_modifier(&self) -> bool {
        if self.cur.token != Token::Identifier {
            return false;
        }
        let f = self.cfg.features;
        match self.name_text() {
            n if SOFT_MODIFIER_NAMES.contains(&n) => true,
            "erased" => f.contains(Features::ERASED_DEFINITIONS),
            "tracked" => f.contains(Features::MODULARITY),
            "update" | "consume" => f.contains(Features::CAPTURE_CHECKING),
            _ => false,
        }
    }

    /// Whether the current token is in a position where a modifier is allowed.
    pub fn in_modifier_position(&mut self) -> bool {
        let mut la = self.lookahead_scanner();
        loop {
            la.next_token();
            if !(la.cur.token == Token::Newline || la.is_soft_modifier()) {
                return MODIFIER_FOLLOWERS.contains(la.cur.token);
            }
        }
    }

    pub fn is_soft_modifier_in_modifier_position(&mut self) -> bool {
        self.is_soft_modifier() && self.in_modifier_position()
    }

    pub fn is_soft_modifier_in_param_modifier_position(&mut self) -> bool {
        self.is_soft_modifier()
            && !matches!(self.lookahead().token, Token::ColonOp | Token::ColonFollow | Token::ColonEol)
    }
}
