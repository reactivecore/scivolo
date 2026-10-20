// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/CharArrayReader.scala
//   compiler/src/dotty/tools/dotc/parsing/Scanners.scala
//   compiler/src/dotty/tools/dotc/parsing/xml/MarkupParsers.scala
//! Character access for the XML literal parser, which reads the source past the scanner's
//! tokens and resumes scanning afterwards.

use super::reader::Reader;
use super::{Scanner, TokenData};
use crate::source::Offset;
use crate::token::Token;

/// A saved reader position.
pub struct XmlMark<'a>(Reader<'a>);

impl<'a> Scanner<'a> {
    /// The current character.
    pub fn xml_ch(&self) -> char {
        self.reader.ch
    }

    pub fn xml_next_char(&mut self) {
        self.reader.next_char();
    }

    /// The offset of the current character (dotty's `lastCharOffset`).
    pub fn xml_offset(&self) -> Offset {
        self.reader.last_char_offset
    }

    /// The source after the current character, as read raw.
    pub fn xml_rest(&self) -> &'a str {
        self.reader.text.get(self.reader.char_offset as usize..).unwrap_or("")
    }

    pub fn xml_mark(&self) -> XmlMark<'a> {
        XmlMark(self.reader.clone())
    }

    pub fn xml_reset(&mut self, mark: XmlMark<'a>) {
        self.reader = mark.0;
    }

    /// Continue scanning after `td`: make it the current token and read the next one.
    pub fn resume(&mut self, td: TokenData) {
        self.cur = td;
        if self.next.token != Token::Empty && !self.diags.has_errors() {
            self.error("unexpected end of input: possible missing '}' in XML block", self.cur.offset);
        }
        self.next_token();
    }

    /// Scan a Scala block embedded in XML, as if a `{` preceded the current character.
    pub fn xml_open_block(&mut self) {
        let lbrace = TokenData {
            token: Token::LBrace,
            offset: self.reader.ch_offset(),
            last_offset: self.cur.last_offset,
            line_offset: self.cur.line_offset,
            ..TokenData::EMPTY
        };
        self.resume(lbrace);
    }
}
