//! The scanner, a port of dotty's `Scanners.Scanner`.

pub(crate) mod chars;
mod comment;
mod fetch;
mod ident;
mod indent;
mod infix;
mod literal;
mod lookahead;
mod newline;
mod number;
mod post;
mod reader;
mod region;
mod skip;
mod string;
mod token_data;
mod xml;

pub use chars::{is_identifier_part, is_identifier_start, is_operator_part, is_xml_name_start};
pub use indent::IndentWidth;
pub use region::{RegionId, RegionKind, Regions};
pub use token_data::TokenData;

use crate::config::Config;
use crate::diag::Diagnostics;
use crate::names::Names;
use crate::source::{Offset, SourceFile};
use crate::token::Token;
use literal::LitBuf;
use reader::Reader;

pub struct Scanner<'a> {
    pub source: &'a SourceFile,
    pub names: &'a mut Names,
    pub diags: Diagnostics,
    pub cfg: Config,
    /// The current token.
    pub cur: TokenData,
    /// The token after the current one, if already read.
    pub next: TokenData,
    prev: TokenData,
    /// The values of string and character literals, in UTF-16 units.
    pub strings: Vec<Box<[u16]>>,
    pub regions: Regions,
    pub region: RegionId,
    reader: Reader<'a>,
    lit: LitBuf,
    indent_syntax: bool,
    allow_leading_infix: bool,
    check_end_markers: bool,
    skipping: bool,
    /// The low half of a supplementary character reported as illegal, still to be reported,
    /// and the line start before it.
    pending_low_surrogate: Option<(u16, Offset)>,
}

impl<'a> Scanner<'a> {
    pub fn new(source: &'a SourceFile, names: &'a mut Names, cfg: Config) -> Self {
        Self::start(source, names, cfg, 0, true)
    }

    fn start(
        source: &'a SourceFile,
        names: &'a mut Names,
        cfg: Config,
        from: Offset,
        allow_indent: bool,
    ) -> Self {
        let mut regions = Regions::default();
        let region =
            regions.push(RegionKind::Indented { width: IndentWidth::zero(), prefix: Token::Empty }, None);
        let mut s = Scanner {
            source,
            names,
            diags: Diagnostics::default(),
            cfg,
            cur: TokenData::EMPTY,
            next: TokenData::EMPTY,
            prev: TokenData::EMPTY,
            strings: Vec::new(),
            regions,
            region,
            reader: Reader::new(&source.text, from),
            lit: LitBuf::default(),
            indent_syntax: cfg.indent_syntax() && allow_indent,
            allow_leading_infix: true,
            check_end_markers: true,
            skipping: false,
            pending_low_surrogate: None,
        };
        s.reader.next_char();
        s.next_token();
        s.region = s.top_level_region(IndentWidth::at(source, s.cur.offset));
        s
    }

    fn top_level_region(&mut self, width: IndentWidth) -> RegionId {
        self.regions.push(RegionKind::Indented { width, prefix: Token::Empty }, None)
    }

    pub fn token(&self) -> Token {
        self.cur.token
    }

    pub fn offset(&self) -> Offset {
        self.cur.offset
    }

    /// The text of the current token's name.
    pub fn name_text(&self) -> &str {
        self.cur.name.map_or("", |n| self.names.text(n))
    }

    /// The string value of `td`.
    pub fn str_val(&self, td: &TokenData) -> Option<&[u16]> {
        td.str_val.map(|i| &*self.strings[i as usize])
    }

    /// Read the next token.
    pub fn next_token(&mut self) {
        let last_token = self.cur.token;
        let last_name = self.cur.name;
        self.adjust_sep_regions(last_token);
        self.get_next_token(last_token);
        if self.cur.is_after_line_end() {
            self.handle_new_line(last_token);
        }
        self.post_process_token(last_token, last_name);
    }

    /// Read the next token and return the offset of the current one.
    pub fn skip_token(&mut self) -> Offset {
        let off = self.cur.offset;
        self.next_token();
        off
    }

    fn get_next_token(&mut self, last_token: Token) {
        if self.next.token == Token::Empty {
            self.cur.last_offset = self.reader.last_char_offset;
            match self.regions.get(self.region).kind {
                RegionKind::InString { multi_line } if last_token != Token::StringPart => {
                    self.fetch_string_part(multi_line)
                }
                _ => self.fetch_token(),
            }
            if self.cur.token == Token::Error {
                self.adjust_sep_regions(Token::StringLit);
            }
        } else {
            self.cur = self.next;
            self.next.token = Token::Empty;
        }
    }

    pub fn adjust_sep_regions(&mut self, last_token: Token) {
        let r = self.region;
        match last_token {
            Token::LParen | Token::LBracket => {
                self.region = self.regions.push(RegionKind::InParens { prefix: last_token }, Some(r))
            }
            Token::LBrace => self.region = self.regions.push(RegionKind::InBraces, Some(r)),
            Token::RBrace => {
                self.drop_until(|k| matches!(k, RegionKind::InBraces));
                if !self.regions.is_outermost(self.region) {
                    self.region = self.regions.enclosing(self.region);
                }
            }
            Token::RParen | Token::RBracket => {
                if let RegionKind::InParens { prefix } = self.regions.get(r).kind
                    && prefix.closer() == last_token
                {
                    self.region = self.regions.enclosing(r);
                }
            }
            Token::Outdent if self.regions.is_indented(r) && !self.regions.is_outermost(r) => {
                self.region = self.regions.enclosing(r)
            }
            Token::StringLit if matches!(self.regions.get(r).kind, RegionKind::InString { .. }) => {
                self.region = self.regions.enclosing(r)
            }
            _ => {}
        }
    }

    /// Report an error and make the current token erroneous.
    fn error(&mut self, message: impl Into<String>, offset: Offset) {
        self.diags.error(offset, message);
        self.cur.token = Token::Error;
    }

    /// Report an error at the end of the input and end the token stream.
    fn incomplete_input_error(&mut self, message: impl Into<String>) {
        self.diags.error(self.cur.offset, message);
        self.cur.token = Token::Eof;
    }
}
