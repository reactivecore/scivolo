use super::scan::{char_len, is_ident_start};
use super::{Lexer, Region, Tok, Token};

impl Lexer<'_> {
    fn triple(&self) -> bool {
        self.peek(0) == b'"' && self.peek(1) == b'"' && self.peek(2) == b'"'
    }

    fn close_triple(&mut self) {
        self.pos += 3;
        while self.peek(0) == b'"' {
            self.pos += 1;
        }
    }

    pub(super) fn string(&mut self) -> Tok {
        if self.triple() {
            self.pos += 3;
            while self.pos < self.src.len() && !self.triple() {
                self.pos += 1;
            }
            if self.pos >= self.src.len() {
                self.errors += 1;
                return Tok::Error;
            }
            self.close_triple();
            return Tok::Str;
        }
        self.pos += 1;
        loop {
            match self.peek(0) {
                b'"' => break,
                b'\\' => self.pos += 2,
                b'\n' | 0 => {
                    self.errors += 1;
                    return Tok::Error;
                }
                _ => self.pos += 1,
            }
        }
        self.pos += 1;
        Tok::Str
    }

    pub(super) fn interp_start(&mut self) -> Tok {
        let triple = self.triple();
        self.pos += if triple { 3 } else { 1 };
        self.regions.push(Region::Interp { triple });
        Tok::InterpStart
    }

    pub(super) fn interp_part(&mut self, triple: bool) -> Token {
        let start = self.pos;
        match self.peek(0) {
            b'$' if self.peek(1) == b'{' => {
                self.pos += 2;
                self.regions.push(Region::Splice);
                return self.token(Tok::InterpSplice, start);
            }
            b'$' if matches!(self.peek(1), b'$' | b'"') => {}
            b'$' if self.pos + 1 < self.src.len() && is_ident_start(self.src, self.pos + 1) => {
                self.pos += 1;
                while self.pos < self.src.len()
                    && self.peek(0) != b'$'
                    && (self.peek(0).is_ascii_digit() || is_ident_start(self.src, self.pos))
                {
                    self.pos += char_len(self.peek(0));
                }
                return self.token(Tok::Ident, start);
            }
            b'"' if !triple || self.triple() => {
                if triple {
                    self.close_triple()
                } else {
                    self.pos += 1
                }
                self.regions.pop();
                return self.token(Tok::InterpEnd, start);
            }
            _ => {}
        }
        loop {
            match self.peek(0) {
                b'$' if self.peek(1) == b'$' || self.peek(1) == b'"' => self.pos += 2,
                b'$' => break,
                b'"' if !triple || self.triple() => break,
                b'\\' if !triple => self.pos += 2,
                b'\n' if !triple => break,
                0 if self.pos >= self.src.len() => break,
                _ => self.pos += 1,
            }
        }
        if self.pos == start {
            self.pos += 1;
            self.errors += 1;
            self.regions.pop();
            return self.token(Tok::Error, start);
        }
        self.token(Tok::InterpPart, start)
    }
}
