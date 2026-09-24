//! Throughput-oriented Scala 3 lexer: tokens, string interpolation and indentation regions.

mod layout;
mod scan;
mod string;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    Ident,
    Keyword,
    Op,
    Int,
    Float,
    Char,
    Str,
    Quote,
    InterpStart,
    InterpPart,
    InterpSplice,
    InterpEnd,
    Open,
    Close,
    Comma,
    Semi,
    Dot,
    Newline,
    Indent,
    Outdent,
    Eof,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub kind: Tok,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Region {
    Indent(u32),
    Brace(u32),
    Paren,
    Splice,
    Interp { triple: bool },
}

const UNSET: u32 = u32::MAX;

pub struct Lexer<'s> {
    src: &'s [u8],
    pos: usize,
    line_start: usize,
    regions: Vec<Region>,
    pending: Vec<Token>,
    last: Tok,
    last_span: (usize, usize),
    pub errors: usize,
}

impl<'s> Lexer<'s> {
    pub fn new(src: &'s str) -> Lexer<'s> {
        Lexer {
            src: src.as_bytes(),
            pos: 0,
            line_start: 0,
            regions: vec![Region::Indent(0)],
            pending: Vec::new(),
            last: Tok::Newline,
            last_span: (0, 0),
            errors: 0,
        }
    }

    fn peek(&self, ahead: usize) -> u8 {
        self.src.get(self.pos + ahead).copied().unwrap_or(0)
    }

    fn token(&self, kind: Tok, start: usize) -> Token {
        Token { kind, start: start as u32, end: self.pos as u32 }
    }

    fn next_token(&mut self) -> Token {
        if let Some(&Region::Interp { triple }) = self.regions.last() {
            return self.interp_part(triple);
        }
        let newline = self.skip_trivia();
        if newline && let Some(tok) = self.layout() {
            return tok;
        }
        let start = self.pos;
        let Some(&b) = self.src.get(self.pos) else { return self.eof() };
        let kind = match b {
            b'(' | b'[' => self.open(Region::Paren),
            b'{' => self.open(Region::Brace(UNSET)),
            b')' | b']' | b'}' => self.close(b),
            b',' => self.single(Tok::Comma),
            b';' => self.single(Tok::Semi),
            b'.' if !self.peek(1).is_ascii_digit() => self.single(Tok::Dot),
            b'0'..=b'9' | b'.' => self.number(),
            b'"' => self.string(),
            b'\'' => self.quote(),
            b'`' => self.backquoted(),
            _ if scan::is_ident_start(self.src, self.pos) => self.ident(),
            _ if scan::is_op_char(self.src, self.pos) => self.op(),
            _ => {
                self.pos += scan::char_len(b);
                self.errors += 1;
                Tok::Error
            }
        };
        self.token(kind, start)
    }

    fn single(&mut self, kind: Tok) -> Tok {
        self.pos += 1;
        kind
    }

    fn open(&mut self, region: Region) -> Tok {
        self.pos += 1;
        self.regions.push(region);
        Tok::Open
    }

    fn close(&mut self, b: u8) -> Tok {
        self.pos += 1;
        let brace = b == b'}';
        while let Some(region) = self.regions.pop() {
            match region {
                Region::Indent(_) if self.regions.is_empty() => {
                    self.regions.push(region);
                    self.errors += 1;
                    return Tok::Error;
                }
                Region::Indent(_) => continue,
                Region::Splice | Region::Brace(_) if brace => return Tok::Close,
                Region::Paren if !brace => return Tok::Close,
                _ => {
                    self.errors += 1;
                    return Tok::Error;
                }
            }
        }
        Tok::Error
    }

    fn eof(&mut self) -> Token {
        let start = self.pos;
        while let Some(Region::Indent(col)) = self.regions.last().copied() {
            if col == 0 {
                break;
            }
            self.regions.pop();
            self.pending.push(self.token(Tok::Outdent, start));
        }
        if self.regions.len() > 1 {
            self.errors += 1;
            self.regions.truncate(1);
        }
        self.pending.insert(0, self.token(Tok::Eof, start));
        self.pending.pop().expect("eof queued")
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.last == Tok::Eof {
            return None;
        }
        let tok = self.pending.pop().unwrap_or_else(|| self.next_token());
        if !matches!(tok.kind, Tok::Newline | Tok::Indent | Tok::Outdent) {
            self.last = tok.kind;
            self.last_span = (tok.start as usize, tok.end as usize);
        }
        if tok.kind == Tok::Eof {
            self.last = Tok::Eof;
        }
        Some(tok)
    }
}
