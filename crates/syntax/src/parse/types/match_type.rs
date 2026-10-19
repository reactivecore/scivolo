// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
use super::super::Parser;
use crate::ast::{NodeId, Span, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// `t match { TypeCaseClauses }`.
    pub fn match_type(&mut self, t: NodeId) -> NodeId {
        let start = self.start_offset(t);
        let point = self.accept(Token::Match);
        let cases = self.in_braces_or_indented(|p| p.case_clauses(Self::type_case_clause));
        let cases = self.ast.list(&cases);
        let m = self.ast.node(Tree::MatchTypeTree { bound: None, selector: t, cases });
        self.at_span2(start, point, m)
    }

    /// `case (InfixType | _) => Type [semi]`.
    pub fn type_case_clause(&mut self) -> NodeId {
        let start = self.offset();
        let pat = self.in_case(|p| {
            let w = p.s.indent_width(p.offset());
            p.s.regions.propose_known_width(p.s.region, &w, Token::Case);
            p.accept(Token::Case);
            if p.tok() == Token::Uscore && matches!(p.s.lookahead().token, Token::Arrow | Token::CtxArrow) {
                let start = p.s.skip_token();
                let name = p.type_name("_");
                let id = p.ast.node(Tree::Ident { name });
                let span = Span::new(start, p.last_offset(), start);
                p.ast.with_span(id, span);
                id
            } else {
                let saved = std::mem::replace(&mut p.in_match_pattern, true);
                let t = p.infix_type();
                p.in_match_pattern = saved;
                p.reject_wildcard_type(t)
            }
        });
        let arrow = self.accept(Token::Arrow);
        let t = self.typ();
        let t = self.reject_wildcard_type(t);
        self.s.observe_outdented();
        if self.tok() == Token::Semi {
            self.next();
        }
        self.new_lines_opt_when_followed_by(Token::Case);
        let body = self.at_span1(arrow, t);
        let c = self.ast.node(Tree::CaseDef { pat, guard: None, body });
        self.at_span1(start, c)
    }
}
