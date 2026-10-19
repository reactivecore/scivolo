// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Lambdas after a `:` (fewer braces).

use super::{Location, Parser};
use crate::ast::NodeId;
use crate::config::Features;
use crate::scan::{RegionKind, Scanner};
use crate::token::Token;

/// How to parse the lambda after a `:` (dotty's `followingIsLambdaAfterColon` results).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColonLambda {
    /// An indented lambda body.
    Indented,
    /// A lambda on the rest of the line.
    SingleLine,
    /// A case clause on the rest of the line.
    SingleCase,
}

/// Parameters (an identifier, `_`, or a parenthesized or bracketed list) and an arrow.
fn params_and_arrow(la: &mut Scanner, relaxed: bool, in_parens: bool) -> Option<ColonLambda> {
    la.next_token();
    if la.cur.is_ident() || la.cur.token == Token::Uscore {
        la.next_token();
        arrow_indent(la, relaxed, in_parens)
    } else if matches!(la.cur.token, Token::LParen | Token::LBracket) {
        la.skip_parens(true);
        arrow_indent(la, relaxed, in_parens)
    } else if la.cur.token == Token::Case && relaxed {
        Some(ColonLambda::SingleCase)
    } else {
        None
    }
}

fn arrow_indent(la: &mut Scanner, relaxed: bool, in_parens: bool) -> Option<ColonLambda> {
    if !matches!(la.cur.token, Token::Arrow | Token::CtxArrow) {
        return None;
    }
    la.observe_arrow_indented();
    if matches!(la.cur.token, Token::Indent | Token::Eof) {
        Some(ColonLambda::Indented)
    } else if relaxed {
        params_and_arrow(la, relaxed, in_parens).or((!in_parens).then_some(ColonLambda::SingleLine))
    } else {
        None
    }
}

impl Parser<'_> {
    /// After `:`: a lambda whose parameters and `=>` follow, and how to parse its body.
    pub fn following_is_lambda_after_colon(&mut self) -> Option<ColonLambda> {
        let relaxed = self.s.cfg.features.contains(Features::RELAXED_LAMBDA_SYNTAX);
        let in_parens = matches!(self.s.regions.get(self.s.region).kind, RegionKind::InParens { .. });
        let width = self.s.regions.indent_width(self.s.region);
        let mut la = self.s.lookahead_scanner_with(true);
        let r = la.region;
        la.regions.get_mut(r).known_width = Some(width);
        params_and_arrow(&mut la, relaxed, in_parens)
    }

    pub fn detect_colon_lambda(&mut self) -> Option<ColonLambda> {
        if self.s.cfg.source_version.enables_fewer_braces() && self.tok() == Token::ColonFollow {
            self.following_is_lambda_after_colon()
        } else {
            None
        }
    }

    /// Parse the body of a lambda after `:` found by `detect_colon_lambda`.
    pub fn colon_lambda(&mut self, kind: ColonLambda) -> NodeId {
        match kind {
            ColonLambda::Indented => self.expr_at(Location::InColonArg),
            ColonLambda::SingleLine => {
                let t = self.in_sep_region(
                    |p, r| p.push_region(RegionKind::SingleLineLambda, r),
                    |p| p.expr_at(Location::InColonArg),
                );
                self.accept(Token::EndLambda);
                t
            }
            ColonLambda::SingleCase => {
                let t = self.in_sep_region(
                    |p, r| p.push_region(RegionKind::SingleLineLambda, r),
                    |p| p.single_case_match(),
                );
                self.accept(Token::EndLambda);
                t
            }
        }
    }
}
