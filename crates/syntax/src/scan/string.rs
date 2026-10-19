// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Scanners.scala
//! String literals, multi-line strings and the parts of interpolated strings.

use super::Scanner;
use super::chars::is_unicode_identifier_part;
use super::chars::is_unicode_identifier_start;
use super::ident::Target;
use super::reader::{CR, LF, SU};
use super::region::RegionKind;
use crate::token::Token;

impl Scanner<'_> {
    /// At `"`: a string, a multi-line string, or the first part of an interpolated string.
    pub fn fetch_double_quote(&mut self) {
        if self.cur.token == Token::InterpolationId {
            self.reader.next_raw_char();
            if self.reader.ch == '"' {
                if self.reader.lookahead_char() == '"' {
                    self.reader.next_raw_char();
                    self.reader.next_raw_char();
                    self.string_part(true);
                } else {
                    self.reader.next_char();
                    self.empty_string();
                }
            } else {
                self.string_part(false);
            }
        } else {
            self.reader.next_char();
            if self.reader.ch == '"' {
                self.reader.next_char();
                if self.reader.ch == '"' {
                    self.reader.next_raw_char();
                    self.get_raw_string_lit();
                } else {
                    self.empty_string();
                }
            } else {
                self.get_string_lit();
            }
        }
    }

    fn string_part(&mut self, multi_line: bool) {
        self.get_string_part(multi_line);
        self.region = self.regions.push(RegionKind::InString { multi_line }, Some(self.region));
    }

    fn empty_string(&mut self) {
        self.cur.token = Token::StringLit;
        self.push_str_val(Vec::new());
    }

    fn get_string_lit(&mut self) {
        self.get_lit_chars('"');
        if self.reader.ch == '"' {
            self.set_str_val();
            self.reader.next_char();
            self.cur.token = Token::StringLit;
        } else {
            self.error("unclosed string literal", self.cur.offset);
        }
    }

    pub fn get_raw_string_lit(&mut self) {
        loop {
            match self.reader.ch {
                '"' => {
                    self.reader.next_raw_char();
                    if self.is_triple_quote() {
                        self.set_str_val();
                        self.cur.token = Token::StringLit;
                        return;
                    }
                }
                SU => return self.incomplete_input_error("unclosed multi-line string literal"),
                ch => {
                    self.put_char(ch);
                    self.reader.next_raw_char();
                }
            }
        }
    }

    /// Read an interpolated string up to its end (STRINGLIT) or a splice (STRINGPART).
    pub fn get_string_part(&mut self, multi_line: bool) {
        loop {
            let ch = self.reader.ch;
            if ch == '"' {
                if multi_line {
                    self.reader.next_raw_char();
                    if !self.is_triple_quote() {
                        continue;
                    }
                } else {
                    self.reader.next_char();
                }
                self.set_str_val();
                self.cur.token = Token::StringLit;
                return;
            } else if ch == '\\' && !multi_line {
                self.put_char(ch);
                self.reader.next_raw_char();
                if matches!(self.reader.ch, '"' | '\\') {
                    self.put_char(self.reader.ch);
                    self.reader.next_raw_char();
                }
            } else if ch == '$' {
                self.reader.next_raw_char();
                let c = self.reader.ch;
                if c == '$' || c == '"' {
                    self.put_char(c);
                    self.reader.next_raw_char();
                } else if c == '{' {
                    self.set_str_val();
                    self.cur.token = Token::StringPart;
                    return;
                } else if is_unicode_identifier_start(c) || c == '_' {
                    return self.get_interpolated_ident();
                } else {
                    let at = self.reader.ch_offset() - 1;
                    self.error(
                        "invalid string interpolation: `$$`, `$\"`, `$`ident or `$`BlockExpr expected",
                        at,
                    );
                    self.put_char('$');
                }
            } else if ch == SU || !multi_line && (ch == CR || ch == LF) {
                if multi_line {
                    return self.incomplete_input_error("unclosed multi-line string literal");
                }
                return self.error("unclosed string literal", self.cur.offset);
            } else {
                self.put_char(ch);
                self.reader.next_raw_char();
            }
        }
    }

    /// After `$`: end the part and read the identifier into the next token.
    fn get_interpolated_ident(&mut self) {
        self.set_str_val();
        self.cur.token = Token::StringPart;
        let at = self.reader.ch_offset();
        self.next.last_offset = at;
        self.next.offset = at;
        self.put_char(self.reader.ch);
        self.reader.next_raw_char();
        while self.reader.ch != SU && is_unicode_identifier_part(self.reader.ch) {
            self.put_char(self.reader.ch);
            self.reader.next_raw_char();
        }
        self.finish_named_token(Token::Identifier, Target::Next);
    }

    pub fn fetch_string_part(&mut self, multi_line: bool) {
        self.cur.offset = self.reader.ch_offset();
        self.get_string_part(multi_line);
    }

    /// After a `"`: whether it is the second of three, consuming extra closing quotes.
    fn is_triple_quote(&mut self) -> bool {
        if self.reader.ch != '"' {
            self.put_char('"');
            return false;
        }
        self.reader.next_raw_char();
        if self.reader.ch != '"' {
            self.put_char('"');
            self.put_char('"');
            return false;
        }
        self.reader.next_char();
        while self.reader.ch == '"' {
            self.put_char('"');
            self.reader.next_char();
        }
        true
    }
}
