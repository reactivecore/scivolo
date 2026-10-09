//! Numeric literals.

use super::Scanner;
use super::chars::{digit, is_identifier_part};
use crate::token::Token;

impl Scanner<'_> {
    /// Read a number into the string value and set its base and token.
    pub fn get_number(&mut self) {
        let mut base = 10;
        if self.reader.ch == '0' {
            self.reader.next_char();
            match self.reader.ch {
                'x' | 'X' => base = 16,
                'b' | 'B' => base = 2,
                _ => self.put_char('0'),
            }
            if base != 10 {
                self.reader.next_char();
                if self.reader.ch != '_' && digit(self.reader.ch, base).is_none() {
                    self.error("invalid literal number", self.cur.offset);
                }
            }
        }
        self.cur.base = base as u8;
        while self.reader.ch == '_' || digit(self.reader.ch, base).is_some() {
            self.put_char(self.reader.ch);
            self.reader.next_char();
        }
        self.check_no_trailing_separator();
        self.cur.token = Token::IntLit;
        if base == 10 && self.reader.ch == '.' {
            if self.reader.lookahead_char().is_ascii_digit() {
                self.put_char('.');
                self.reader.next_char();
                self.get_fraction();
            }
        } else {
            match self.reader.ch {
                'e' | 'E' | 'f' | 'F' | 'd' | 'D' if base == 10 => self.get_fraction(),
                'l' | 'L' => {
                    self.reader.next_char();
                    self.cur.token = Token::LongLit;
                }
                _ => {}
            }
        }
        self.check_no_trailing_separator();
        self.set_str_val();
    }

    /// Read the fractional part and exponent of a floating point number.
    pub fn get_fraction(&mut self) {
        self.cur.token = Token::DeciLit;
        self.put_digits();
        if matches!(self.reader.ch, 'e' | 'E') {
            let mut la = self.reader.clone();
            la.next_char();
            if matches!(la.ch, '+' | '-') {
                la.next_char();
            }
            if la.ch.is_ascii_digit() || self.reader.ch == '_' {
                self.put_char(self.reader.ch);
                self.reader.next_char();
                if matches!(self.reader.ch, '+' | '-') {
                    self.put_char(self.reader.ch);
                    self.reader.next_char();
                }
                self.put_digits();
            }
            self.cur.token = Token::ExpoLit;
        }
        match self.reader.ch {
            'd' | 'D' => self.finish_suffix(Token::DoubleLit),
            'f' | 'F' => self.finish_suffix(Token::FloatLit),
            _ => {}
        }
        let ch = self.reader.ch;
        if is_identifier_part(ch) && (' '..='\u{FFFF}').contains(&ch) {
            self.error("Invalid literal number", self.cur.offset);
        }
    }

    fn put_digits(&mut self) {
        while self.reader.ch.is_ascii_digit() || self.reader.ch == '_' {
            self.put_char(self.reader.ch);
            self.reader.next_char();
        }
        self.check_no_trailing_separator();
    }

    fn finish_suffix(&mut self, token: Token) {
        self.put_char(self.reader.ch);
        self.reader.next_char();
        self.cur.token = token;
    }

    fn check_no_trailing_separator(&mut self) {
        if self.lit.last() == Some(b'_' as u16) {
            let at = self.cur.offset + self.lit.len16() as u32 - 1;
            self.diags.error(at, "trailing separator is not allowed");
        }
    }
}
