//! `match` clauses, case clauses and guards.

use super::super::{Location, Parser};
use crate::ast::tree::MatchKind;
use crate::ast::{NodeId, Span, Tree};
use crate::config::Features;
use crate::scan::RegionKind;
use crate::token::Token;

impl Parser<'_> {
    /// Drop an `InCase` region directly enclosing the current one (for sub-matches in guards).
    fn drop_inner_case_region(&mut self) {
        let r = self.s.region;
        let region = self.s.regions.get(r);
        match &region.kind {
            RegionKind::Indented { width, prefix } => {
                if let Some(o) = region.outer
                    && self.s.regions.get(o).kind == RegionKind::InCase
                {
                    let (kind, outer) = (
                        RegionKind::Indented { width: width.clone(), prefix: *prefix },
                        self.s.regions.get(o).outer,
                    );
                    self.s.region = self.s.regions.push(kind, outer);
                }
            }
            RegionKind::InCase => self.s.region = self.s.regions.enclosing(r),
            _ => {}
        }
    }

    /// `t match { CaseClauses }`.
    pub fn match_clause(&mut self, t: NodeId) -> NodeId {
        let start = self.start_offset(t);
        let point = self.s.skip_token();
        let cases = if self.s.cfg.features.contains(Features::SUB_CASES) {
            self.drop_inner_case_region();
            if self.tok() == Token::Case {
                vec![self.case_clause(true)]
            } else {
                self.in_braces_or_indented(|p| p.case_clauses(|p| p.case_clause(false)))
            }
        } else {
            self.in_braces_or_indented(|p| p.case_clauses(|p| p.case_clause(false)))
        };
        let cases = self.ast.list(&cases);
        let m = self.ast.node(Tree::Match { selector: Some(t), cases, kind: MatchKind::Plain });
        self.at_span2(start, point, m)
    }

    pub fn case_clauses(&mut self, mut clause: impl FnMut(&mut Self) -> NodeId) -> Vec<NodeId> {
        let mut cases = vec![clause(self)];
        while self.tok() == Token::Case {
            cases.push(clause(self));
        }
        cases
    }

    pub fn in_case<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        self.in_sep_region(|p, r| p.push_region(RegionKind::InCase, r), body)
    }

    /// `case Pattern [Guard] => Block` (or `=> Expr` for `expr_only`).
    pub fn case_clause(&mut self, expr_only: bool) -> NodeId {
        let start = self.offset();
        let (pat, grd) = self.in_case(|p| {
            let w = p.s.indent_width(p.offset());
            p.s.regions.propose_known_width(p.s.region, &w, Token::Case);
            p.accept(Token::Case);
            let saved = std::mem::replace(&mut p.in_match_pattern, true);
            let pat = p.pattern(Location::InPattern);
            p.in_match_pattern = saved;
            (pat, p.guard())
        });
        let mut grd1 = grd;
        let tok = self.tok();
        let body = match tok {
            Token::Arrow => {
                let point = self.s.skip_token();
                let b = if expr_only {
                    if self.tok() == Token::EndLambda {
                        self.s.cur.token = Token::Newline;
                        self.s.observe_indented();
                    }
                    if self.s.cfg.indent_syntax()
                        && self.s.cur.is_after_line_end()
                        && self.tok() != Token::Indent
                    {
                        let msg = "Misleading indentation: this expression forms part of the preceding case.\nIf this is intended, it should be indented for clarity.\nOtherwise, if the handler is intended to be empty, use a multi-line match or catch with\nan indented case.";
                        self.warning(msg, self.offset());
                    }
                    self.expr()
                } else {
                    self.block(false)
                };
                self.at_span1(point, b)
            }
            Token::If => {
                let offset = self.s.skip_token();
                let g = self.in_case(|p| {
                    let w = p.s.indent_width(offset);
                    p.s.regions.propose_known_width(p.s.region, &w, Token::If);
                    p.postfix_expr(Location::InGuard)
                });
                let b = self.as_sub_match(g, tok);
                self.at_span1(offset, b)
            }
            _ => {
                let g = grd1.take();
                match g {
                    Some(g) => self.as_sub_match(g, tok),
                    None => self.missing_arrow_body(tok, None),
                }
            }
        };
        let c = self.ast.node(Tree::CaseDef { pat, guard: grd1, body });
        self.at_span1(start, c)
    }

    /// A guard read as the sub-match of a case, or an error.
    fn as_sub_match(&mut self, t: NodeId, tok: Token) -> NodeId {
        if let Tree::Match { selector, cases, .. } = *self.ast.tree(t)
            && self.s.cfg.features.contains(Features::SUB_CASES)
        {
            if self.s.cur.is_stat_sep() {
                self.next();
            }
            return self.ast.node(Tree::Match { selector, cases, kind: MatchKind::Sub });
        }
        self.missing_arrow_body(tok, Some(t))
    }

    fn missing_arrow_body(&mut self, tok: Token, t: Option<NodeId>) -> NodeId {
        let msg = Self::expected_but_found(Token::Arrow, tok, "", "");
        self.syntax_error_or_incomplete(msg, self.offset());
        let stats = self.ast.list(&[]);
        let b = self.ast.node(Tree::Block { stats, expr: None });
        let span = t.map_or(Span::NONE, |t| self.ast.span(t));
        self.at_span(span, b)
    }

    /// `match` with a single case clause, for `catch case` and single-line lambdas.
    pub fn single_case_match(&mut self) -> NodeId {
        let c = self.case_clause(true);
        let cases = self.ast.list(&[c]);
        self.ast.node(Tree::Match { selector: None, cases, kind: MatchKind::Plain })
    }

    /// `if PostfixExpr`, or nothing.
    pub fn guard(&mut self) -> Option<NodeId> {
        if self.tok() == Token::If {
            self.next();
            Some(self.postfix_expr(Location::InGuard))
        } else {
            None
        }
    }
}
