//! Interpolated strings.

use super::{Location, Parser};
use crate::ast::constant::Constant;
use crate::ast::{NodeId, Span, TName, Tree};
use crate::diag::Message;
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// `id"..."`: parts and spliced arguments as pairs of a literal and an argument.
    pub fn interpolated_string(&mut self, in_pattern: bool, start: Offset) -> NodeId {
        let interpolator = TName { name: self.s.cur.name.expect("an interpolator"), is_type: false };
        let text = self.s.source.text.as_bytes();
        let quote_end = self.s.cur.offset as usize + self.s.name_text().len();
        let triple = text.get(quote_end + 1) == Some(&b'"') && text.get(quote_end + 2) == Some(&b'"');
        self.next();
        let mut segments = Vec::new();
        let mut correction = if triple { 3 } else { 1 };
        while self.tok() == Token::StringPart {
            let lit = self.interpolation_literal(self.offset() + correction);
            let arg = self.interpolation_arg(in_pattern);
            let trees = self.ast.list(&[lit, arg]);
            segments.push(self.ast.node(Tree::Thicket { trees }));
            correction = 0;
        }
        if self.tok() == Token::StringLit {
            segments.push(self.interpolation_literal(self.offset() + correction));
        }
        let segments = self.ast.list(&segments);
        let t = self.ast.node(Tree::InterpolatedString { id: interpolator, segments });
        self.at_span1(start, t)
    }

    fn interpolation_literal(&mut self, start: Offset) -> NodeId {
        let t = match self.tok() {
            Token::StringLit | Token::StringPart => {
                let value: Box<[u16]> = self.str_val().into();
                let end = self.s.source.advance_utf16(start, value.len());
                let t = self.ast.node(Tree::Literal { value: Constant::String(value) });
                self.at_span(Span::new(start, end, start), t)
            }
            _ => {
                self.syntax_error_or_incomplete(Message::syntax(28, "Illegal literal"), self.offset());
                let t = self.ast.node(Tree::Literal { value: Constant::Null });
                self.at_span1(start, t)
            }
        };
        self.next();
        t
    }

    /// The argument after `$`, or an empty tree after an error.
    fn interpolation_arg(&mut self, in_pattern: bool) -> NodeId {
        let start = self.offset();
        let t = match self.tok() {
            Token::Identifier => self.term_ident(),
            Token::Uscore if in_pattern => {
                self.next();
                let name = self.term("_");
                self.ast.node(Tree::Ident { name })
            }
            Token::This => {
                self.next();
                self.ast.node(Tree::This { qual: None })
            }
            Token::LBrace if in_pattern => {
                let pat = self.in_braces(|p| p.pattern(Location::InPattern));
                let stats = self.ast.list(&[]);
                self.ast.node(Tree::Block { stats, expr: Some(pat) })
            }
            Token::LBrace => self.expr(),
            _ => {
                let o = self.offset();
                let msg = Message::syntax(16, "Error in interpolated string: identifier or block expected");
                self.s.diags.error_at(o, o, o, msg);
                return self.empty_tree();
            }
        };
        self.at_span1(start, t)
    }
}
