//! One token and where it is (dotty's `TokenData`).

use crate::names::Name;
use crate::source::Offset;
use crate::token::Token;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TokenData {
    pub token: Token,
    pub offset: Offset,
    /// The offset after the token preceding this one.
    pub last_offset: Offset,
    /// The offset of the line break before this token, if it starts a line.
    pub line_offset: Option<Offset>,
    pub name: Option<Name>,
    /// An index into `Scanner::strings`.
    pub str_val: Option<u32>,
    pub base: u8,
}

impl TokenData {
    pub const EMPTY: TokenData = TokenData {
        token: Token::Empty,
        offset: 0,
        last_offset: 0,
        line_offset: None,
        name: None,
        str_val: None,
        base: 0,
    };

    pub fn is_new_line(&self) -> bool {
        matches!(self.token, Token::Newline | Token::Newlines)
    }

    pub fn is_stat_sep(&self) -> bool {
        self.is_new_line() || self.token == Token::Semi
    }

    pub fn is_ident(&self) -> bool {
        matches!(self.token, Token::Identifier | Token::BackquotedIdent)
    }

    pub fn is_colon(&self) -> bool {
        matches!(self.token, Token::ColonOp | Token::ColonFollow | Token::ColonEol)
    }

    pub fn is_arrow(&self) -> bool {
        matches!(self.token, Token::Arrow | Token::CtxArrow)
    }

    pub fn is_nested_start(&self) -> bool {
        matches!(self.token, Token::LBrace | Token::Indent)
    }

    pub fn is_nested_end(&self) -> bool {
        matches!(self.token, Token::RBrace | Token::Outdent)
    }

    pub fn is_after_line_end(&self) -> bool {
        self.line_offset.is_some()
    }
}
