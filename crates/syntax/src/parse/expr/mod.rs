//! Expressions: `Expr`, conditions and parts of statements.

mod args;
mod ascription;
mod cases;
mod closure;
mod control;
mod convert_params;
mod for_expr;
mod new_expr;
mod operators;
mod simple;

use super::migration::Migration;
use super::{Location, Parser};
use crate::ast::NodeId;
use crate::ast::modifiers::Modifiers;
use crate::config::Features;
use crate::scan::RegionKind;
use crate::token::Token;
use crate::token::sets::TokenSet;

impl Parser<'_> {
    pub fn expr(&mut self) -> NodeId {
        self.expr_at(Location::Elsewhere)
    }

    pub fn expr_in_parens(&mut self) -> NodeId {
        self.expr_at(Location::InParens)
    }

    pub fn sub_expr(&mut self) -> NodeId {
        self.sub_part(|p| p.expr())
    }

    /// `Expr`: a lambda, a polymorphic function literal, or `Expr1`, with `_` placeholders
    /// turned into the parameters of a function.
    pub fn expr_at(&mut self, location: Location) -> NodeId {
        self.nested(|p| p.expr_at_level(location))
    }

    fn expr_at_level(&mut self, location: Location) -> NodeId {
        let start = self.offset();
        match self.tok() {
            Token::Implicit => {
                let msg = "`implicit` lambdas are no longer supported, use a lambda with `?=>` instead";
                self.migration_error(msg, self.offset(), Migration::Scala2Implicits);
                let mods = self.modifiers(TokenSet::of(&[Token::Implicit]), Modifiers::default());
                self.closure(start, location, mods)
            }
            Token::LBracket => {
                let start = self.offset();
                let tparams = self.type_param_clause(super::defs::ParamOwner::Type);
                let arrow = self.accept(Token::Arrow);
                let body = self.expr_at(location);
                self.make_poly_function(tparams, body, "literal", start, arrow, |p, arrow| {
                    p.error_term_tree(arrow)
                })
            }
            Token::Case if self.s.cfg.features.contains(Features::RELAXED_LAMBDA_SYNTAX) => {
                self.single_case_match()
            }
            _ => {
                let saved = std::mem::take(&mut self.placeholder_params);
                let t = self.expr1(location);
                if matches!(self.tok(), Token::Arrow | Token::CtxArrow) {
                    let in_case = self.s.regions.get(self.s.region).kind == RegionKind::InCase;
                    if in_case && location == Location::InBlock {
                        self.check_non_param_tuple(t);
                        self.wrap_placeholders(t, saved)
                    } else {
                        self.placeholder_params.clear();
                        let params = self.convert_to_params(t);
                        let c = self.closure_rest(start, location, params);
                        self.wrap_placeholders(c, saved)
                    }
                } else if self.is_wildcard(t) {
                    let current = std::mem::replace(&mut self.placeholder_params, saved);
                    self.placeholder_params.extend(current);
                    t
                } else {
                    self.check_non_param_tuple(t);
                    self.wrap_placeholders(t, saved)
                }
            }
        }
    }

    /// `t` as the body of a function of the placeholder parameters, if there are any; then
    /// restore `saved`.
    fn wrap_placeholders(&mut self, t: NodeId, saved: Vec<NodeId>) -> NodeId {
        let params = std::mem::replace(&mut self.placeholder_params, saved);
        if params.is_empty() {
            return t;
        }
        let args = self.ast.list(&params);
        self.ast.node(crate::ast::Tree::Function { args, body: t, mods: None, wildcard: true })
    }

    /// Whether the current conditional continues after the initial `(...)`.
    fn cond_continues(&mut self, alt: Token) -> bool {
        if self.s.cur.is_new_line() || self.s.cfg.source_version.migrate_to_3() {
            return false;
        }
        !self.s.can_start_stat_tokens().contains(self.tok()) || self.followed_by_token(alt)
    }

    /// The condition of `if` or `while`, up to `alt` (`then` or `do`).
    pub fn cond_expr(&mut self, alt: Token) -> NodeId {
        let t = if self.tok() == Token::LParen {
            let t = self.in_sep_region(Self::in_old_cond, |p| {
                let start = p.offset();
                let ts = p.in_parens_with_commas(|p| p.comma_separated(|p| p.expr_in_parens()));
                let t = p.make_tuple_or_parens(&ts);
                p.at_span1(start, t)
            });
            if self.tok() == alt {
                t
            } else if self.cond_continues(alt) {
                self.in_sep_region(
                    |p, r| p.push_region(RegionKind::InParens { prefix: Token::LParen }, r),
                    |p| {
                        let t = p.simple_expr_rest(t, Location::Elsewhere, true);
                        let t = p.postfix_expr_rest(t, Location::Elsewhere);
                        p.expr1_rest(t, Location::Elsewhere)
                    },
                )
            } else {
                let span = self.ast.span(t);
                self.rewrite_to_new_syntax(span);
                self.s.observe_indented();
                return t;
            }
        } else if matches!(self.tok(), Token::LBrace | Token::Indent) {
            let t = self.expr();
            self.new_lines_opt();
            t
        } else {
            self.in_sep_region(
                |p, r| p.push_region(RegionKind::InParens { prefix: Token::LParen }, r),
                |p| p.expr(),
            )
        };
        let start = self.ast.span(t).start_pos();
        self.rewrite_to_old_syntax(start);
        self.accept(alt);
        t
    }

    /// An old-style condition region: an indented region inside `(`, so an indented body
    /// can follow (dotty's `InOldCond`).
    fn in_old_cond(&mut self, r: crate::scan::RegionId) -> crate::scan::RegionId {
        if let RegionKind::InParens { prefix } = self.s.regions.get(r).kind {
            let width = self.s.regions.indent_width(r);
            self.push_region(RegionKind::Indented { width, prefix }, r)
        } else {
            r
        }
    }

    /// Parse `body`; under `-no-indent`, warn if a `{` seems to be missing before it.
    pub fn sub_part(&mut self, body: impl FnOnce(&mut Self) -> NodeId) -> NodeId {
        let r = self.s.region;
        if self.s.regions.get(r).kind != RegionKind::InBraces || !self.s.cur.is_after_line_end() {
            return body(self);
        }
        let start_width = self.s.indent_width(self.offset());
        if !self.s.regions.indent_width(r).lt(&start_width) {
            return body(self);
        }
        let t = body(self);
        if self.s.cur.is_new_line() {
            let next_width = self.s.indent_width(self.s.next.offset);
            if !next_width.lt(&start_width) {
                let msg = if start_width.le(&next_width) {
                    let text = "Line is indented too far to the right, or a '{' is missing before:\n\n";
                    crate::diag::Message::syntax(229, text)
                } else {
                    format!(
                        "Incompatible combinations of tabs and spaces in indentation prefixes.\nPrevious indent : {start_width}\nLatest indent   : {next_width}"
                    )
                    .into()
                };
                let o = self.s.next.offset;
                self.warning(msg, o);
            }
        }
        t
    }
}
