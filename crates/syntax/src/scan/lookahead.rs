//! Looking past the current token, and inserting tokens before it.

use super::{Scanner, TokenData};
use crate::source::Offset;
use crate::token::Token;

impl Scanner<'_> {
    /// A scanner starting at the current token, without significant indentation.
    pub fn lookahead_scanner(&mut self) -> Scanner<'_> {
        Scanner::start(self.source, self.names, self.cfg, self.cur.offset, false)
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
