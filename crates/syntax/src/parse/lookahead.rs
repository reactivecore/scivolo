//! Deciding between alternatives by scanning ahead (dotty's `followingIs...`).

use super::Parser;
use crate::config::Features;
use crate::token::Token;
use crate::token::sets::{CAN_START_INFIX_TYPE_TOKENS, STOP_SCAN_TOKENS, TokenSet};

impl Parser<'_> {
    /// Whether `query` comes before anything that starts, separates or continues a statement.
    pub fn followed_by_token(&mut self, query: Token) -> bool {
        let mut la = self.s.lookahead_scanner();
        let mut braces = 0;
        loop {
            let token = la.cur.token;
            if query != Token::LArrow && token == Token::XmlStart {
                return false;
            }
            let nested_end = matches!(token, Token::RBrace | Token::Outdent);
            if braces == 0 {
                if token == query {
                    return true;
                }
                if STOP_SCAN_TOKENS.contains(token) || nested_end {
                    return false;
                }
            } else if token == Token::Eof {
                return false;
            } else if nested_end {
                braces -= 1;
            }
            if matches!(token, Token::LBrace | Token::Indent) {
                braces += 1;
            }
            la.next_token();
        }
    }

    /// Whether `(...)` after `for` holds generators.
    pub fn following_is_enclosed_generators(&mut self) -> bool {
        let mut la = self.s.lookahead_scanner();
        let mut parens = 1;
        la.next_token();
        while parens != 0 && la.cur.token != Token::Eof {
            match la.cur.token {
                Token::XmlStart => return true,
                Token::LParen => parens += 1,
                Token::RParen => parens -= 1,
                _ => {}
            }
            la.next_token();
        }
        if la.cur.token == Token::LArrow {
            false
        } else if la.cur.is_ident() {
            true
        } else {
            self.followed_by_token(Token::LArrow)
        }
    }

    /// Whether a named given with a `:` follows.
    pub fn following_is_given_def_with_colon(&mut self) -> bool {
        let new_givens = self.s.cfg.source_version.enables_new_givens();
        let mut la = self.s.lookahead_scanner();
        if la.cur.is_ident() {
            la.next_token();
        }
        fn skip_params(la: &mut crate::scan::Scanner) {
            loop {
                if matches!(la.cur.token, Token::LParen | Token::LBracket) {
                    la.skip_parens(true);
                } else if la.cur.is_new_line() {
                    la.next_token();
                } else {
                    return;
                }
            }
        }
        skip_params(&mut la);
        if !matches!(la.cur.token, Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
            return false;
        }
        if !new_givens {
            return true;
        }
        la.next_token();
        if !la.cur.is_after_line_end() {
            return true;
        }
        la.cur.is_ident() && {
            la.next_token();
            skip_params(&mut la);
            matches!(la.cur.token, Token::With | Token::Equals)
        }
    }

    pub fn following_is_arrow(&mut self) -> bool {
        let mut la = self.s.lookahead_scanner();
        la.skip_parens(true);
        la.cur.token == Token::Arrow
    }

    pub fn following_is_extension(&mut self) -> bool {
        matches!(self.s.lookahead().token, Token::LBracket | Token::LParen)
    }

    pub fn following_is_self_type(&mut self) -> bool {
        let mut la = self.s.lookahead_scanner_with(true);
        la.next_token();
        la.cur.token == Token::ColonFollow && {
            la.observe_colon_eol(false);
            la.next_token();
            CAN_START_INFIX_TYPE_TOKENS.contains(la.cur.token)
        }
    }

    /// Whether the current `*` ends an argument list (or precedes a `,` under multiSpreads).
    pub fn following_is_vararg(&mut self) -> bool {
        if !self.is_ident_named("*") {
            return false;
        }
        let multi = self.s.cfg.features.contains(Features::MULTI_SPREADS);
        let mut la = self.s.lookahead_scanner();
        la.next_token();
        la.cur.token == Token::RParen
            || la.cur.token == Token::Comma
                && (multi || {
                    la.next_token();
                    matches!(la.cur.token, Token::RParen | Token::Eof)
                })
    }

    /// At `:` inside parentheses: whether the parameters of a lambda are being parsed.
    pub fn following_is_lambda_params(&mut self) -> bool {
        let mut la = self.s.lookahead_scanner();
        la.next_token();
        while !matches!(la.cur.token, Token::RParen | Token::Eof) {
            if la.cur.token == Token::LParen { la.skip_parens(true) } else { la.next_token() }
        }
        la.cur.token == Token::RParen && {
            la.next_token();
            matches!(la.cur.token, Token::Arrow | Token::CtxArrow)
        }
    }

    /// Whether the token after the current one can start an operand, so the current
    /// identifier can be an infix operator.
    pub fn next_can_follow_operator(&mut self, leading_operand: TokenSet) -> bool {
        let next = self.s.lookahead().token;
        leading_operand.contains(next)
            || self.s.cfg.features.contains(Features::POSTFIX_OPS)
            || next == Token::ColonOp
            || next == Token::Eof
    }
}
