//! Reading one token from the characters (dotty's `fetchToken`).

use super::Scanner;
use super::chars::{
    is_identifier_start, is_operator_part, is_special, is_unicode_identifier_start, is_xml_name_start,
};
use super::reader::{CR, FF, LF, SU};
use super::region::RegionKind;
use crate::token::Token;

impl Scanner<'_> {
    pub fn fetch_token(&mut self) {
        loop {
            self.cur.offset = self.reader.ch_offset();
            self.cur.line_offset = (self.cur.last_offset < self.reader.line_start_offset)
                .then_some(self.reader.line_start_offset);
            self.cur.name = None;
            if let Some((low, line_start)) = self.pending_low_surrogate.take() {
                self.cur.line_offset = (self.cur.last_offset < line_start).then_some(line_start);
                return self.error(format!("illegal character '\\u{low:04x}'"), self.cur.offset);
            }
            let ch = self.reader.ch;
            match ch {
                ' ' | '\t' | CR | LF | FF => {
                    self.reader.next_char();
                    continue;
                }
                'A'..='Z' | 'a'..='z' | '$' | '_' => {
                    self.put_char(ch);
                    self.reader.next_char();
                    self.get_ident_rest();
                    self.mark_interpolation_id();
                }
                '<' => self.fetch_lt(),
                '~' | '!' | '@' | '#' | '%' | '^' | '*' | '+' | '-' | '>' | '?' | ':' | '=' | '&' | '|'
                | '\\' => {
                    self.put_char(ch);
                    self.reader.next_char();
                    self.get_operator_rest();
                }
                '/' => {
                    if self.skip_comment() {
                        continue;
                    }
                    self.put_char('/');
                    self.get_operator_rest();
                }
                '0'..='9' => self.get_number(),
                '`' => self.get_backquoted_ident(),
                '"' => self.fetch_double_quote(),
                '\'' => self.fetch_single_quote(),
                '.' => {
                    self.reader.next_char();
                    if self.reader.ch.is_ascii_digit() {
                        self.put_char('.');
                        self.get_fraction();
                        self.set_str_val();
                    } else {
                        self.cur.token = Token::Dot;
                    }
                }
                ';' | ',' | '(' | '{' | ')' | '[' | ']' => {
                    self.reader.next_char();
                    self.cur.token = match ch {
                        ';' => Token::Semi,
                        ',' => Token::Comma,
                        '(' => Token::LParen,
                        '{' => Token::LBrace,
                        ')' => Token::RParen,
                        '[' => Token::LBracket,
                        _ => Token::RBracket,
                    };
                }
                '}' => {
                    let in_string = matches!(self.regions.get(self.region).kind, RegionKind::InBraces)
                        && self.regions.get(self.region).outer.is_some_and(|o| {
                            self.regions.get(o).kind == RegionKind::InString { multi_line: true }
                        });
                    if in_string {
                        self.reader.next_raw_char()
                    } else {
                        self.reader.next_char()
                    }
                    self.cur.token = Token::RBrace;
                }
                SU if self.reader.is_at_end() => self.cur.token = Token::Eof,
                SU => {
                    self.error("illegal character", self.cur.offset);
                    self.reader.next_char();
                }
                _ => self.fetch_other(),
            }
            return;
        }
    }

    fn mark_interpolation_id(&mut self) {
        if self.reader.ch == '"' && self.cur.token == Token::Identifier {
            self.cur.token = Token::InterpolationId;
        }
    }

    fn fetch_lt(&mut self) {
        let last = self.reader.byte_before_ch().unwrap_or(b' ');
        self.reader.next_char();
        let ch = self.reader.ch;
        if matches!(last, b' ' | b'\t' | b'\n' | b'{' | b'(' | b'>')
            && (is_xml_name_start(ch) || ch == '!' || ch == '?')
        {
            self.cur.token = Token::XmlStart;
        } else {
            self.put_char('<');
            self.get_operator_rest();
        }
    }

    fn fetch_single_quote(&mut self) {
        self.reader.next_char();
        let ch = self.reader.ch;
        let bmp = (ch as u32) <= 0xFFFF;
        if bmp && is_identifier_start(ch) {
            self.char_lit_or(Self::get_ident_rest);
        } else if bmp && is_operator_part(ch) && ch != '\\' {
            self.char_lit_or(Self::get_operator_rest);
        } else if matches!(ch, '{' | '[' | ' ' | '\t') && self.reader.lookahead_char() != '\'' {
            self.cur.token = Token::Quote;
        } else if !self.reader.is_at_end() && !matches!(ch, SU | CR | LF) {
            let empty = ch == '\'';
            self.get_lit_char();
            if self.reader.ch == '\'' {
                if empty {
                    self.error("empty character literal (use '\\'' for single quote)", self.cur.offset);
                } else if self.lit.len16() != 1 {
                    let units: String = self.lit.units().iter().map(|u| format!("\\u{u:04x}")).collect();
                    self.error(format!("illegal codepoint in Char constant: '{units}'"), self.cur.offset);
                } else {
                    self.finish_char_lit();
                }
            } else if empty {
                self.error("empty character literal", self.cur.offset);
            } else {
                self.error("unclosed character literal", self.cur.offset);
            }
        } else {
            self.error("unclosed character literal", self.cur.offset);
        }
    }

    fn fetch_other(&mut self) {
        let ch = self.reader.ch;
        if ch == '\u{21D2}' || ch == '\u{2190}' {
            self.reader.next_char();
            let (token, ascii) = if ch == '\u{21D2}' { (Token::Arrow, "=>") } else { (Token::LArrow, "<-") };
            self.cur.token = token;
            self.diags.warning(
                self.cur.offset,
                format!("The unicode arrow `{ch}` is deprecated, use `{ascii}` instead. If you still wish to display it as one character, consider using a font with programming ligatures such as Fira Code."),
            );
        } else if is_unicode_identifier_start(ch) {
            self.put_char(ch);
            self.reader.next_char();
            self.get_ident_rest();
            self.mark_interpolation_id();
        } else if is_special(ch) {
            self.put_char(ch);
            self.reader.next_char();
            self.get_operator_rest();
        } else {
            let units = ch.encode_utf16(&mut [0; 2]).to_vec();
            self.error(format!("illegal character '\\u{:04x}'", units[0]), self.cur.offset);
            self.pending_low_surrogate = units.get(1).map(|&u| (u, self.reader.line_start_offset));
            self.reader.next_char();
        }
    }
}
