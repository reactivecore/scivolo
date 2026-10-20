// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Scanners.scala
//! Looking past the current token, and inserting tokens before it.

use super::{Scanner, TokenData};
use crate::source::Offset;
use crate::token::Token;

impl Scanner<'_> {
    /// A scanner starting at the current token, without significant indentation.
    pub fn lookahead_scanner(&mut self) -> Scanner<'_> {
        self.lookahead_scanner_with(false)
    }

    pub fn lookahead_scanner_with(&mut self, allow_indent: bool) -> Scanner<'_> {
        Scanner::start(self.source, self.names, self.cfg, self.cur.offset, allow_indent)
    }

    /// The offset of the last character the reader read.
    pub fn last_char_offset(&self) -> Offset {
        self.reader.last_char_offset
    }

    /// Insert `token` at `offset` in front of the current one.
    pub fn insert(&mut self, token: Token, offset: Offset) {
        debug_assert!(self.next.token == Token::Empty);
        self.next = self.cur;
        self.cur.offset = offset;
        self.cur.token = token;
    }

    /// The token after the current one; newlines and indentation are not inserted.
    pub fn lookahead(&mut self) -> TokenData {
        if self.next.token == Token::Empty {
            debug_assert!(self.cur.token != Token::InterpolationId);
            self.peek_ahead();
            self.reset();
        }
        self.next
    }

    /// Read the next token into `cur`, keeping the current one for `reset`.
    pub fn peek_ahead(&mut self) {
        self.prev = self.cur;
        self.get_next_token(self.cur.token);
        if self.cur.token == Token::End && !self.is_end_marker() {
            self.cur.token = Token::Identifier;
        }
    }

    /// Undo `peek_ahead`, keeping the peeked token as `next`.
    pub fn reset(&mut self) {
        self.next = self.cur;
        self.cur = self.prev;
    }
}
