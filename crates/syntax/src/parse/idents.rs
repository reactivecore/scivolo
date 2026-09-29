//! Identifiers, selections and simple references.

use super::Parser;
use crate::ast::{Attrs, NodeId, Span, TName, Tree};
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// Accept an identifier and return its term name, or `<error>`.
    pub fn ident(&mut self) -> TName {
        if self.is_ident() {
            let name = self.s.cur.name.expect("an identifier has a name");
            if matches!(self.s.names.text(name), "<init>" | "<clinit>") {
                let o = self.offset();
                self.s.diags.error(o, "Illegal backquoted identifier: `<init>` and `<clinit>` are forbidden");
            }
            self.next();
            TName { name, is_type: false }
        } else {
            let msg = Self::expected_but_found(Token::Identifier, self.tok(), "", "");
            self.syntax_error_or_incomplete(msg, self.offset());
            self.term("<error>")
        }
    }

    pub fn term_ident(&mut self) -> NodeId {
        let (tok, offset) = (self.tok(), self.offset());
        let name = self.ident();
        self.make_ident(tok, offset, name)
    }

    pub fn type_ident(&mut self) -> NodeId {
        let (tok, offset) = (self.tok(), self.offset());
        let name = self.ident();
        self.make_ident(tok, offset, TName { is_type: true, ..name })
    }

    pub fn make_ident(&mut self, tok: Token, offset: Offset, name: TName) -> NodeId {
        let t = self.ast.node(Tree::Ident { name });
        if tok == Token::BackquotedIdent {
            self.ast.add_attrs(t, Attrs::BACKQUOTED);
        }
        let error_offset = offset.min(self.last_offset().saturating_sub(1));
        if self.s.names.text(name.name) == "<error>" && !self.ast.span(t).exists() {
            self.ast.with_span(t, Span::synthetic(error_offset, error_offset));
            t
        } else {
            self.at_span1(offset, t)
        }
    }

    pub fn wildcard_ident(&mut self) -> NodeId {
        let start = self.accept(Token::Uscore);
        let name = self.term("_");
        let t = self.ast.node(Tree::Ident { name });
        self.at_span1(start, t)
    }

    /// `t.id`, or `t match {...}` at `match`.
    pub fn selector_or_match(&mut self, t: NodeId) -> NodeId {
        let (start, point) = (self.start_offset(t), self.offset());
        let sel = if self.tok() == Token::Match {
            self.match_clause(t)
        } else {
            let name = self.ident();
            self.ast.node(Tree::Select { qualifier: t, name })
        };
        self.at_span2(start, point, sel)
    }

    pub fn selector(&mut self, t: NodeId) -> NodeId {
        let (start, point) = (self.start_offset(t), self.offset());
        let name = self.ident();
        let sel = self.ast.node(Tree::Select { qualifier: t, name });
        self.at_span2(start, point, sel)
    }

    /// `{. id}` after `t`.
    pub fn dot_selectors(&mut self, mut t: NodeId) -> NodeId {
        while self.tok() == Token::Dot {
            self.next();
            t = self.selector(t);
        }
        t
    }

    /// `id`, `[id .] this`, or `[id .] super [ClassQualifier] . id`.
    pub fn simple_ref(&mut self) -> NodeId {
        let start = self.offset();
        if self.tok() == Token::This {
            return self.handle_this(start, None);
        }
        if self.tok() == Token::Super {
            return self.handle_super(start, None);
        }
        if self.tok() != Token::InterpolationId && self.s.lookahead().token == Token::Dot {
            let (tok, offset) = (self.tok(), self.offset());
            let name = self.ident();
            match self.s.lookahead().token {
                Token::This => {
                    self.next();
                    let qual = self.make_ident(tok, offset, TName { is_type: true, ..name });
                    self.handle_this(start, Some(qual))
                }
                Token::Super => {
                    self.next();
                    let qual = self.make_ident(tok, offset, TName { is_type: true, ..name });
                    self.handle_super(start, Some(qual))
                }
                _ => self.make_ident(tok, offset, name),
            }
        } else {
            self.term_ident()
        }
    }

    fn handle_this(&mut self, start: Offset, qual: Option<NodeId>) -> NodeId {
        self.next();
        let t = self.ast.node(Tree::This { qual });
        self.at_span1(start, t)
    }

    fn handle_super(&mut self, start: Offset, qual: Option<NodeId>) -> NodeId {
        self.next();
        let mix = self.mixin_qualifier_opt();
        let this = self.ast.node(Tree::This { qual });
        let sup = self.ast.node(Tree::Super { qual: this, mix });
        let t = self.at_span1(start, sup);
        self.accept(Token::Dot);
        self.selector(t)
    }

    /// `[id]` after `super`.
    pub fn mixin_qualifier_opt(&mut self) -> Option<NodeId> {
        if self.tok() != Token::LBracket {
            return None;
        }
        Some(self.in_brackets(|p| {
            let start = p.offset();
            let id = p.type_ident();
            p.at_span1(start, id)
        }))
    }

    /// `id {. id}`.
    pub fn qual_id(&mut self) -> NodeId {
        let id = self.term_ident();
        self.dot_selectors(id)
    }

    /// An identifier or selection turned into a type name.
    pub fn convert_to_type_id(&mut self, t: NodeId) -> NodeId {
        match &mut self.ast.get_mut(t).tree {
            Tree::Ident { name } | Tree::Select { name, .. } => {
                name.is_type = true;
                t
            }
            _ => {
                let span = self.ast.span(t);
                self.syntax_error_span("identifier expected", span);
                t
            }
        }
    }
}
