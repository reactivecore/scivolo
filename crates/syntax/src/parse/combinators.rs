//! Parsing combinators: enclosing tokens, comma-separated parts, separator regions.

use super::Parser;
use crate::ast::{NodeId, Tree};
use crate::scan::{RegionId, RegionKind};
use crate::token::Token;
use crate::token::sets::{CAN_START_EXPR_TOKENS3, CAN_START_TYPE_TOKENS};

impl Parser<'_> {
    /// `tok` body `tok.closer()`.
    pub fn enclosed<T>(&mut self, tok: Token, body: impl FnOnce(&mut Self) -> T) -> T {
        self.accept(tok);
        let t = body(self);
        self.accept(tok.closer());
        t
    }

    /// Like `enclosed`, but a missing closer suggests a `,` where one could follow.
    pub fn enclosed_with_commas<T>(&mut self, tok: Token, body: impl FnOnce(&mut Self) -> T) -> T {
        self.accept(tok);
        let closing = tok.closer();
        let is_empty = self.tok() == closing;
        let ts = body(self);
        if self.tok() != closing {
            let follow = if tok == Token::LParen { CAN_START_EXPR_TOKENS3 } else { CAN_START_TYPE_TOKENS };
            let prefix = if !is_empty && follow.contains(self.tok()) { "',' or " } else { "" };
            let msg = Self::expected_but_found(closing, self.tok(), prefix, "");
            self.syntax_error_or_incomplete(msg, self.offset());
        }
        if self.tok() == closing {
            self.next();
        }
        ts
    }

    pub fn in_parens<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        self.enclosed(Token::LParen, body)
    }

    pub fn in_braces<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        self.enclosed(Token::LBrace, body)
    }

    pub fn in_brackets<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        self.enclosed(Token::LBracket, body)
    }

    pub fn in_parens_with_commas<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        self.enclosed_with_commas(Token::LParen, body)
    }

    pub fn in_brackets_with_commas<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        self.enclosed_with_commas(Token::LBracket, body)
    }

    /// `{ body }` or an indented `body`.
    pub fn in_braces_or_indented<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        if self.tok() == Token::Indent { self.enclosed(Token::Indent, body) } else { self.in_braces(body) }
    }

    /// `part {, part}`, with commas expected in the current region.
    pub fn comma_separated(&mut self, mut part: impl FnMut(&mut Self) -> NodeId) -> Vec<NodeId> {
        let r = self.s.region;
        self.s.regions.get_mut(r).commas_expected = true;
        let first = part(self);
        let ts = self.comma_separated_rest(first, part);
        self.s.regions.get_mut(r).commas_expected = false;
        ts
    }

    /// `{, part}` after `leading`.
    pub fn comma_separated_rest(
        &mut self,
        leading: NodeId,
        mut part: impl FnMut(&mut Self) -> NodeId,
    ) -> Vec<NodeId> {
        let mut ts = vec![leading];
        while self.tok() == Token::Comma {
            self.next();
            ts.push(part(self));
        }
        ts
    }

    /// `name = part` as a named argument where named tuples are enabled, else `part`.
    pub fn maybe_named(&mut self, part: impl FnOnce(&mut Self) -> NodeId) -> NodeId {
        if self.is_ident()
            && self.s.lookahead().token == Token::Equals
            && self.s.cfg.source_version.enables_named_tuples()
        {
            let start = self.offset();
            let name = self.ident();
            self.next();
            let arg = part(self);
            let t = self.ast.node(Tree::NamedArg { name, arg });
            self.at_span1(start, t)
        } else {
            part(self)
        }
    }

    /// Parse `body` with the current region replaced by `f(current)`.
    pub fn in_sep_region<T>(
        &mut self,
        f: impl FnOnce(&mut Self, RegionId) -> RegionId,
        body: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let cur = self.s.region;
        self.s.region = f(self, cur);
        let t = body(self);
        self.s.region = cur;
        t
    }

    /// A region of kind `kind` enclosed by `outer`.
    pub fn push_region(&mut self, kind: RegionKind, outer: RegionId) -> RegionId {
        self.s.regions.push(kind, Some(outer))
    }
}
