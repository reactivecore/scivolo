// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! `for` expressions: enumerators, generators, guards and aliases.

use super::super::Parser;
use crate::ast::tree::GenCheckMode;
use crate::ast::{NodeId, Span, Tree};
use crate::config::SourceVersion;
use crate::diag::Message;
use crate::scan::RegionKind;
use crate::token::Token;

impl Parser<'_> {
    fn enumerators(&mut self) -> Vec<NodeId> {
        let mut enums = if self.s.cfg.enables_better_fors() {
            self.aliases_until_generator()
        } else {
            vec![self.generator()]
        };
        self.enumerators_rest(&mut enums);
        enums
    }

    fn enumerators_rest(&mut self, enums: &mut Vec<NodeId>) {
        loop {
            if self.s.cur.is_stat_sep() {
                self.next();
                if matches!(self.tok(), Token::Do | Token::Yield | Token::RBrace) {
                    return;
                }
                let e = self.enumerator();
                enums.push(e);
            } else if self.tok() == Token::If {
                let g = self.guard().expect("a guard");
                enums.push(g);
            } else {
                return;
            }
        }
    }

    fn enumerator(&mut self) -> NodeId {
        match self.tok() {
            Token::If => self.guard().expect("a guard"),
            Token::Case => self.generator(),
            _ => {
                let pat = self.pattern1(super::super::Location::InPattern);
                if self.tok() == Token::Equals {
                    self.gen_alias(pat)
                } else {
                    self.generator_rest(pat, false)
                }
            }
        }
    }

    fn gen_alias(&mut self, pat: NodeId) -> NodeId {
        let start = self.start_offset(pat);
        let point = self.s.skip_token();
        let expr = self.sub_expr();
        let t = self.ast.node(Tree::GenAlias { pat, expr });
        self.at_span2(start, point, t)
    }

    fn generator(&mut self) -> NodeId {
        let case_pat = self.tok() == Token::Case;
        if case_pat {
            self.next();
        }
        let pat = self.pattern1(super::super::Location::InPattern);
        self.generator_rest(pat, case_pat)
    }

    fn generator_rest(&mut self, pat: NodeId, case_pat: bool) -> NodeId {
        let start = self.start_offset(pat);
        let point = self.accept(Token::LArrow);
        let v = self.s.cfg.source_version;
        let check_mode = if case_pat {
            GenCheckMode::FilterAlways
        } else if v.is_at_least(SourceVersion::V3_4) {
            GenCheckMode::Check
        } else if v.is_at_least(SourceVersion::V3_2) {
            GenCheckMode::CheckAndFilter
        } else {
            GenCheckMode::FilterNow
        };
        let expr = self.sub_expr();
        let t = self.ast.node(Tree::GenFrom { pat, expr, check_mode });
        self.at_span2(start, point, t)
    }

    fn aliases_until_generator(&mut self) -> Vec<NodeId> {
        let mut out = Vec::new();
        loop {
            if self.tok() == Token::Case {
                out.push(self.generator());
                return out;
            }
            let pat = self.pattern1(super::super::Location::InPattern);
            if self.tok() != Token::Equals {
                out.push(self.generator_rest(pat, false));
                return out;
            }
            out.push(self.gen_alias(pat));
            if self.s.cur.is_stat_sep() {
                self.next();
            }
        }
    }

    /// `for (enums) [do | yield] expr`, `for {enums} ...`, `for enums (do | yield) expr`.
    pub fn for_expr(&mut self) -> NodeId {
        let for_start = self.s.skip_token();
        let mut wrapped = true;
        let start = self.offset();
        let leading = self.tok();
        let enums = if leading == Token::LBrace
            || leading == Token::LParen && self.following_is_enclosed_generators()
        {
            self.next();
            let res = if leading == Token::LBrace || self.tok() == Token::Case {
                self.enumerators()
            } else {
                let pats = self.patterns_opt(super::super::Location::InPattern);
                let pat = if self.tok() == Token::RParen || pats.len() > 1 {
                    wrapped = false;
                    self.accept(Token::RParen);
                    let t = self.make_tuple_or_parens(&pats);
                    self.at_span1(start, t)
                } else {
                    pats[0]
                };
                let mut enums = vec![self.generator_rest(pat, false)];
                self.enumerators_rest(&mut enums);
                enums
            };
            if wrapped {
                self.accept(leading.closer());
            }
            self.s.observe_indented();
            res
        } else {
            wrapped = false;
            if self.tok() == Token::Indent {
                self.in_braces_or_indented(|p| p.enumerators())
            } else {
                let ts =
                    self.in_sep_region(|p, r| p.push_region(RegionKind::InBraces, r), |p| p.enumerators());
                self.rewrite_to_old_syntax(Span::at(start));
                ts
            }
        };
        self.new_lines_opt();
        let enums_list = self.ast.list(&enums);
        let t = if self.tok() == Token::Yield {
            self.next();
            let expr = self.sub_expr();
            self.ast.node(Tree::ForYield { enums: enums_list, expr })
        } else if self.tok() == Token::Do {
            self.rewrite_to_old_syntax(Span::at(self.offset()));
            self.next();
            let body = self.sub_expr();
            self.ast.node(Tree::ForDo { enums: enums_list, body })
        } else {
            if !wrapped {
                let msg = Message::syntax(20, "yield or do expected");
                self.syntax_error_or_incomplete(msg, self.offset());
            }
            let body = self.expr();
            self.ast.node(Tree::ForDo { enums: enums_list, body })
        };
        self.at_span1(for_start, t)
    }
}
