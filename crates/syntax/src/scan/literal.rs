//! The literal buffer, escape sequences and character literals.

use super::Scanner;
use super::chars::digit;
use super::reader::{CR, LF, SU};
use crate::token::Token;

/// The UTF-16 units of the token being read, as dotty's `litBuf`.
#[derive(Default)]
pub struct LitBuf {
    units: Vec<u16>,
}

impl LitBuf {
    pub fn put(&mut self, c: char) {
        self.units.extend_from_slice(c.encode_utf16(&mut [0; 2]));
    }

    pub fn put_unit(&mut self, u: u16) {
        self.units.push(u);
    }

    pub fn len16(&self) -> usize {
        self.units.len()
    }

    pub fn last(&self) -> Option<u16> {
        self.units.last().copied()
    }

    pub fn take(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.units)
    }

    /// The buffer as a string, lone surrogates replaced, and clear it.
    pub fn take_string(&mut self) -> String {
        String::from_utf16_lossy(&self.take())
    }

    pub fn clear(&mut self) {
        self.units.clear();
    }
}

impl Scanner<'_> {
    pub(super) fn put_char(&mut self, c: char) {
        self.lit.put(c);
    }

    /// Store the buffer as the current token's string value.
    pub(super) fn set_str_val(&mut self) {
        let s = self.lit.take();
        self.push_str_val(s);
    }

    pub(super) fn push_str_val(&mut self, units: Vec<u16>) {
        self.strings.push(units.into_boxed_slice());
        self.cur.str_val = Some(self.strings.len() as u32 - 1);
    }

    /// Copy the current character into the buffer, interpreting an escape sequence.
    pub(super) fn get_lit_char(&mut self) {
        if self.reader.ch == '\\' {
            self.reader.next_char();
            self.char_escape();
        } else {
            self.put_char(self.reader.ch);
            self.reader.next_char();
        }
    }

    fn char_escape(&mut self) {
        let c = match self.reader.ch {
            'b' => '\u{8}',
            't' => '\t',
            'n' => '\n',
            'f' => '\u{C}',
            'r' => '\r',
            '"' => '"',
            '\'' => '\'',
            '\\' => '\\',
            'u' | 'U' => return self.u_escape(),
            '0'..='7' => return self.octal_escape(),
            ch => {
                self.error("invalid escape character", self.reader.ch_offset());
                ch
            }
        };
        self.put_char(c);
        self.reader.next_char();
    }

    fn u_escape(&mut self) {
        while matches!(self.reader.ch, 'u' | 'U') {
            self.reader.next_char();
        }
        let mut cp = 0u32;
        for i in 0..4 {
            let Some(d) = digit(self.reader.ch, 16) else {
                self.error("invalid character in unicode escape sequence", self.reader.ch_offset());
                self.put_char(self.reader.ch);
                return;
            };
            cp += d << ((3 - i) * 4);
            self.reader.next_char();
        }
        self.lit.put_unit(cp as u16);
    }

    fn octal_escape(&mut self) {
        let start = self.reader.ch_offset() - 1;
        let lead = self.reader.ch;
        let mut oct = digit(lead, 8).unwrap_or(0);
        self.reader.next_char();
        if let Some(d) = digit(self.reader.ch, 8) {
            oct = oct * 8 + d;
            self.reader.next_char();
            if lead <= '3'
                && let Some(d) = digit(self.reader.ch, 8)
            {
                oct = oct * 8 + d;
                self.reader.next_char();
            }
        }
        let alt = if oct == 10 { "\\n".to_string() } else { format!("\\u{oct:04x}") };
        self.error(format!("octal escape literals are unsupported: use {alt} instead"), start);
        self.lit.put_unit(oct as u16);
    }

    pub(super) fn get_lit_chars(&mut self, delimiter: char) {
        while self.reader.ch != delimiter
            && !self.reader.is_at_end()
            && !matches!(self.reader.ch, SU | CR | LF)
        {
            self.get_lit_char();
        }
    }

    pub(super) fn finish_char_lit(&mut self) {
        self.reader.next_char();
        self.cur.token = Token::CharLit;
        self.set_str_val();
    }

    /// A character literal if the current character is followed by `'`, else a quoted
    /// identifier read by `rest`.
    pub(super) fn char_lit_or(&mut self, rest: fn(&mut Self)) {
        self.put_char(self.reader.ch);
        self.reader.next_char();
        if self.reader.ch == '\'' {
            self.finish_char_lit();
        } else {
            rest(self);
            let name = self.cur.name.map_or("null", |n| self.names.text(n)).encode_utf16().collect();
            self.cur.token = Token::QuoteId;
            self.push_str_val(name);
            self.lit.clear();
        }
    }
}
