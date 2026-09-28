//! Quotes `'{...}` and `'[...]`, splices `$x` and `${...}`, and the type blocks of quotes.

use super::super::{Parser, stage};
use crate::ast::{NodeId, Span, Tree};
use crate::token::Token;
use crate::token::sets::TokenSet;

impl Parser<'_> {
    fn within_staged<T>(&mut self, kind: u8, body: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.staged;
        self.staged |= kind;
        let t = body(self);
        self.staged = saved;
        t
    }

    /// `{type TypeDef semi} Type`.
    fn type_block(&mut self) -> NodeId {
        let mut tdefs = Vec::new();
        while self.tok() == Token::Type {
            tdefs.push(self.type_block_stat());
        }
        let t = self.typ();
        if tdefs.is_empty() {
            return t;
        }
        let stats = self.ast.list(&tdefs);
        self.ast.node(Tree::Block { stats, expr: Some(t) })
    }

    fn type_block_stat(&mut self) -> NodeId {
        let mods = self.def_annots_mods(TokenSet::of(&[]));
        let start = self.s.skip_token();
        let tdef = self.type_def_or_dcl(start, mods);
        self.s.observe_outdented();
        if self.tok() == Token::Semi {
            self.next();
        }
        if self.s.cur.is_new_line() {
            self.next();
        }
        tdef
    }

    /// `'{ Block }` or `'[ TypeBlock ]`.
    pub fn quote(&mut self, in_pattern: bool) -> NodeId {
        let start = self.s.skip_token();
        let kind = stage::QUOTED | if in_pattern { stage::QUOTED_PATTERN } else { 0 };
        let t = self.within_staged(kind, |p| {
            let body = if p.tok() == Token::LBracket {
                p.in_brackets(|p| p.type_block())
            } else {
                p.in_braces(|p| p.block(true))
            };
            p.ast.node(Tree::Quote { body })
        });
        self.at_span1(start, t)
    }

    /// `$id` inside a quote or `${ Block }` (a pattern inside a quoted pattern).
    pub fn splice(&mut self, is_type: bool) -> NodeId {
        let start = self.offset();
        let in_pattern = self.staged & stage::QUOTED_PATTERN != 0;
        let expr = if self.s.name_text() == "$" {
            self.next();
            self.within_staged(stage::SPLICED, |p| {
                p.in_braces(|p| {
                    if in_pattern { p.pattern(super::super::Location::InPattern) } else { p.block(true) }
                })
            })
        } else {
            let at = self.offset() + 1;
            let name = self.s.name_text()[1..].to_string();
            let name = self.term(&name);
            let id = self.ast.node(Tree::Ident { name });
            self.next();
            self.at_span1(at, id)
        };
        let t = if is_type {
            let hint = if in_pattern {
                "Use lower cased variable name without the `$` instead"
            } else {
                "To use a given Type[T] in a quote just write T directly"
            };
            let msg = format!("Type splicing with `$` in quotes not supported anymore\n\nHint: {hint}");
            let span = Span::synthetic(start, self.last_offset());
            self.syntax_error_span(msg, span);
            let name = self.type_name("<error>");
            self.ast.node(Tree::Ident { name })
        } else if in_pattern {
            self.ast.node(Tree::SplicePattern { body: expr })
        } else {
            self.ast.node(Tree::Splice { expr })
        };
        self.at_span1(start, t)
    }
}
