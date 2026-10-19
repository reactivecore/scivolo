// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Types: function types, type lambdas, match types and infix types.

mod args;
mod bounds;
mod fun_params;
mod function;
mod kind_projector;
mod match_type;
mod quotes;
mod simple;

use super::defs::ParamOwner;
use super::{Location, ParseKind, Parser};
use crate::ast::{NodeId, Span, Tree};
use crate::token::Token;
use crate::token::sets::CAN_START_INFIX_TYPE_TOKENS;
use function::FunType;

impl Parser<'_> {
    /// `typ`, but a wildcard is an error and becomes `scala.Any`.
    pub fn toplevel_typ(&mut self) -> NodeId {
        let t = self.typ();
        self.reject_wildcard_type(t)
    }

    pub fn toplevel_typ_in_context_bound(&mut self) -> NodeId {
        let t = self.typ_in(true);
        self.reject_wildcard_type(t)
    }

    pub fn typ(&mut self) -> NodeId {
        self.typ_in(false)
    }

    fn typ_in(&mut self, in_context_bound: bool) -> NodeId {
        self.nested(|p| p.typ_at_level(in_context_bound))
    }

    fn typ_at_level(&mut self, in_context_bound: bool) -> NodeId {
        let mut ft = FunType::new(self.offset());
        match self.tok() {
            Token::LParen => self.paren_type(&mut ft, in_context_bound),
            Token::LBracket => {
                let start = self.offset();
                let tparams = self.type_param_clause(ParamOwner::Type);
                if self.tok() == Token::TLArrow {
                    let point = self.s.skip_token();
                    let tparams: Vec<NodeId> =
                        tparams.into_iter().map(|t| self.strip_context_bounds(t)).collect();
                    let tparams = self.ast.list(&tparams);
                    let body = self.toplevel_typ();
                    let t = self.ast.node(Tree::LambdaTypeTree { tparams, body });
                    self.at_span2(start, point, t)
                } else if self.tok() == Token::Arrow || self.is_pure_arrow_named("->") {
                    let arrow = self.s.skip_token();
                    let body = self.toplevel_typ();
                    self.make_poly_function(tparams, body, "type", start, arrow, |p, _| {
                        let name = p.type_name("<error>");
                        p.ast.node(Tree::Ident { name })
                    })
                } else {
                    self.accept(Token::TLArrow);
                    self.typ()
                }
            }
            Token::Indent => self.enclosed(Token::Indent, |p| p.typ()),
            _ => {
                let t = self.infix_type_in(in_context_bound);
                self.type_rest(&mut ft, t)
            }
        }
    }

    pub fn infix_type(&mut self) -> NodeId {
        self.infix_type_in(false)
    }

    fn infix_type_in(&mut self, in_context_bound: bool) -> NodeId {
        let t = self.refined_type();
        self.infix_type_rest(t, in_context_bound)
    }

    pub fn infix_type_rest(&mut self, t: NodeId, in_context_bound: bool) -> NodeId {
        self.infix_type_rest_with(t, in_context_bound, |p, _| p.refined_type())
    }

    /// `{id [nl] operand}` after `t`.
    pub fn infix_type_rest_with(
        &mut self,
        t: NodeId,
        in_context_bound: bool,
        operand: fn(&mut Self, Location) -> NodeId,
    ) -> NodeId {
        self.infix_ops(
            t,
            |_, tok| CAN_START_INFIX_TYPE_TOKENS.contains(tok),
            operand,
            Location::Elsewhere,
            ParseKind::Type,
            move |p| {
                !p.following_is_vararg()
                    && !p.is_pure_arrow()
                    && (!in_context_bound
                        || !p.is_ident_named("as")
                        || !p.s.cfg.source_version.enables_new_givens())
                    && p.next_can_follow_operator(CAN_START_INFIX_TYPE_TOKENS)
            },
        )
    }

    /// `: Type`, or an inferred type positioned at the last offset.
    pub fn typed_opt(&mut self) -> NodeId {
        if matches!(self.tok(), Token::ColonOp | Token::ColonFollow | Token::ColonEol) {
            self.next();
            self.toplevel_typ()
        } else {
            let t = self.type_tree();
            let at = self.last_offset();
            self.ast.with_span(t, Span::at(at));
            t
        }
    }

    pub fn type_depending_on(&mut self, location: Location) -> NodeId {
        if location.in_parens() {
            self.typ()
        } else if location.in_pattern() {
            let t = self.refined_type();
            self.reject_wildcard_type(t)
        } else {
            self.infix_type()
        }
    }
}
