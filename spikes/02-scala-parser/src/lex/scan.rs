use super::{Lexer, Tok};

#[rustfmt::skip]
const KEYWORDS: &[&str] = &[
    "abstract", "case", "catch", "class", "def", "do", "else", "enum", "export", "extends", "false",
    "final", "finally", "for", "given", "if", "implicit", "import", "lazy", "match", "new", "null",
    "object", "override", "package", "private", "protected", "return", "sealed", "super", "then", "this",
    "throw", "trait", "true", "try", "type", "val", "var", "while", "with", "yield",
];

pub fn char_len(b: u8) -> usize {
    match b {
        0xF0.. => 4,
        0xE0.. => 3,
        0xC0.. => 2,
        _ => 1,
    }
}

fn decode(src: &[u8], pos: usize) -> char {
    let len = char_len(src[pos]).min(src.len() - pos);
    std::str::from_utf8(&src[pos..pos + len]).ok().and_then(|s| s.chars().next()).unwrap_or('\u{FFFD}')
}

pub fn is_ident_start(src: &[u8], pos: usize) -> bool {
    match src[pos] {
        b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => true,
        0x80.. => decode(src, pos).is_alphabetic(),
        _ => false,
    }
}

fn is_ident_part(src: &[u8], pos: usize) -> bool {
    pos < src.len() && (src[pos].is_ascii_digit() || is_ident_start(src, pos))
}

pub fn is_op_char(src: &[u8], pos: usize) -> bool {
    match src.get(pos) {
        Some(b'!' | b'#' | b'%' | b'&' | b'*' | b'+' | b'-' | b'/' | b':' | b'<' | b'=' | b'>' | b'?') => {
            true
        }
        Some(b'@' | b'\\' | b'^' | b'|' | b'~') => true,
        Some(0x80..) => !decode(src, pos).is_alphanumeric(),
        _ => false,
    }
}

impl Lexer<'_> {
    pub(super) fn skip_trivia(&mut self) -> bool {
        let mut newline = false;
        loop {
            match self.peek(0) {
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'\n' => {
                    self.pos += 1;
                    self.line_start = self.pos;
                    newline = true;
                }
                b'/' if self.peek(1) == b'/' => {
                    while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                b'/' if self.peek(1) == b'*' => self.block_comment(),
                _ => return newline,
            }
        }
    }

    fn block_comment(&mut self) {
        let mut depth = 0;
        while self.pos < self.src.len() {
            match (self.peek(0), self.peek(1)) {
                (b'/', b'*') => (depth, self.pos) = (depth + 1, self.pos + 2),
                (b'*', b'/') => (depth, self.pos) = (depth - 1, self.pos + 2),
                (b'\n', _) => (self.pos, self.line_start) = (self.pos + 1, self.pos + 1),
                _ => self.pos += 1,
            }
            if depth == 0 {
                return;
            }
        }
        self.errors += 1;
    }

    pub(super) fn ident(&mut self) -> Tok {
        let start = self.pos;
        while is_ident_part(self.src, self.pos) {
            self.pos += char_len(self.src[self.pos]);
        }
        if self.src[self.pos - 1] == b'_' && is_op_char(self.src, self.pos) {
            self.op();
        }
        if self.peek(0) == b'"' {
            return self.interp_start();
        }
        let word = &self.src[start..self.pos];
        if KEYWORDS.iter().any(|k| k.as_bytes() == word) { Tok::Keyword } else { Tok::Ident }
    }

    pub(super) fn op(&mut self) -> Tok {
        while is_op_char(self.src, self.pos) {
            if self.peek(0) == b'/' && matches!(self.peek(1), b'/' | b'*') {
                break;
            }
            self.pos += char_len(self.src[self.pos]);
        }
        Tok::Op
    }

    pub(super) fn backquoted(&mut self) -> Tok {
        self.pos += 1;
        while !matches!(self.peek(0), b'`' | b'\n' | 0) {
            self.pos += 1;
        }
        if self.peek(0) != b'`' {
            self.errors += 1;
            return Tok::Error;
        }
        self.pos += 1;
        Tok::Ident
    }

    pub(super) fn number(&mut self) -> Tok {
        if self.peek(0) == b'0' && matches!(self.peek(1), b'x' | b'X' | b'b' | b'B') {
            self.pos += 2;
            self.digits(|b| b.is_ascii_hexdigit());
            return self.int_suffix();
        }
        self.digits(|b| b.is_ascii_digit());
        let mut float = false;
        if self.peek(0) == b'.' && self.peek(1).is_ascii_digit() {
            self.pos += 1;
            self.digits(|b| b.is_ascii_digit());
            float = true;
        }
        if matches!(self.peek(0), b'e' | b'E') {
            let sign = usize::from(matches!(self.peek(1), b'+' | b'-'));
            if self.peek(1 + sign).is_ascii_digit() {
                self.pos += 1 + sign;
                self.digits(|b| b.is_ascii_digit());
                float = true;
            }
        }
        if matches!(self.peek(0), b'f' | b'F' | b'd' | b'D') {
            self.pos += 1;
            return Tok::Float;
        }
        if float { Tok::Float } else { self.int_suffix() }
    }

    fn digits(&mut self, ok: fn(u8) -> bool) {
        while ok(self.peek(0)) || self.peek(0) == b'_' {
            self.pos += 1;
        }
    }

    fn int_suffix(&mut self) -> Tok {
        if matches!(self.peek(0), b'l' | b'L') {
            self.pos += 1;
        }
        Tok::Int
    }

    pub(super) fn quote(&mut self) -> Tok {
        self.pos += 1;
        if self.peek(0) == b'\\' {
            self.pos += 1;
            while !matches!(self.peek(0), b'\'' | b'\n' | 0) {
                self.pos += 1;
            }
        } else if self.peek(0) != 0 && self.src.get(self.pos + char_len(self.peek(0))) == Some(&b'\'') {
            self.pos += char_len(self.peek(0));
        } else {
            return Tok::Quote;
        }
        if self.peek(0) != b'\'' {
            self.errors += 1;
            return Tok::Error;
        }
        self.pos += 1;
        Tok::Char
    }
}
