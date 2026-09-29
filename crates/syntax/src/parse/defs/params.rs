//! Term parameter clauses, and their interleaving with type parameter clauses.

use super::super::Parser;
use super::ParamOwner;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{List, NodeId, Tree};
use crate::config::Features;
use crate::token::Token;
use crate::token::sets::START_PARAM_TOKENS;

impl Parser<'_> {
    /// Interleaved type and term parameter clauses of a method; an `implicit` clause ends them.
    pub fn type_or_term_param_clauses(&mut self, owner: ParamOwner, mut num_lead: usize) -> Vec<List> {
        let (mut out, mut first, mut prev_type) = (Vec::new(), true, false);
        loop {
            self.new_line_opt_when_followed_by(Token::LParen);
            self.new_line_opt_when_followed_by(Token::LBracket);
            if self.tok() == Token::LParen {
                let params = self.term_param_clause(owner, num_lead, first, Modifiers::default());
                let last = self.is_implicit_clause(&params);
                num_lead += params.len();
                out.push(self.ast.list(&params));
                if last {
                    return out;
                }
                first = false;
                prev_type = false;
            } else if self.tok() == Token::LBracket {
                if prev_type {
                    let o = self.offset();
                    self.syntax_error(
                        "Type parameter lists must be separated by a term or using parameter list",
                        o,
                    );
                }
                let tps = self.type_param_clause(owner);
                out.push(self.ast.list(&tps));
                prev_type = true;
            } else {
                return out;
            }
        }
    }

    fn is_implicit_clause(&self, params: &[NodeId]) -> bool {
        params.first().is_some_and(|&p| match self.ast.tree(p) {
            Tree::ValDef { mods, .. } => self.ast.modifiers(*mods).is(flags::IMPLICIT),
            _ => false,
        })
    }

    /// Term parameter clauses; an `implicit` clause ends them.
    pub fn term_param_clauses(&mut self, owner: ParamOwner, mut num_lead: usize) -> Vec<List> {
        let (mut out, mut first) = (Vec::new(), true);
        loop {
            self.new_line_opt_when_followed_by(Token::LParen);
            if self.tok() != Token::LParen {
                return out;
            }
            let params = self.term_param_clause(owner, num_lead, first, Modifiers::default());
            let last = self.is_implicit_clause(&params);
            num_lead += params.len();
            out.push(self.ast.list(&params));
            if last {
                return out;
            }
            first = false;
        }
    }

    /// `(params)`, `(using params)` or `(implicit params)`.
    pub fn term_param_clause(
        &mut self,
        owner: ParamOwner,
        num_lead: usize,
        first: bool,
        initial: Modifiers,
    ) -> Vec<NodeId> {
        let mut implied = initial.clone();
        self.in_parens_with_commas(|p| {
            if p.tok() == Token::RParen && owner != ParamOwner::ExtensionPrefix && !implied.is(flags::GIVEN) {
                if owner.takes_only_using_clauses() {
                    let o = p.offset();
                    p.syntax_error("`using` expected", o);
                }
                return Vec::new();
            }
            let clause = if owner == ParamOwner::ExtensionPrefix
                && !p.is_ident_named("using")
                && !p.is_ident_named("erased")
            {
                vec![p.param(owner, first, &mut implied)]
            } else {
                p.param_mods(&initial, &mut implied);
                if owner.takes_only_using_clauses() && !implied.is(flags::GIVEN) {
                    let o = p.offset();
                    p.syntax_error("`using` expected", o);
                }
                let (first_mod, named) = p.params_are_named(&implied);
                let params = if named {
                    p.comma_separated(|p| p.param(owner, first, &mut implied))
                } else {
                    p.context_types(owner, num_lead, &implied)
                };
                if let Some(&h) = params.first()
                    && let Tree::ValDef { mods, .. } = *p.ast.tree(h)
                {
                    let m = p.ast.modifiers_mut(mods);
                    m.flags = m.flags.or(first_mod.flags);
                }
                params
            };
            p.check_var_args_rules(&clause);
            clause
        })
    }

    /// Whether named parameters follow, and the modifier (`erased`) of the first one.
    fn params_are_named(&mut self, implied: &Modifiers) -> (Modifiers, bool) {
        let mut mods = Modifiers::default();
        if self.tok() != Token::InterpolationId
            && matches!(self.s.lookahead().token, Token::ColonOp | Token::ColonFollow | Token::ColonEol)
        {
            return (mods, true);
        }
        if self.is_erased() {
            mods = self.add_modifier(mods);
        }
        let named = !implied.is(flags::GIVEN)
            || START_PARAM_TOKENS.contains(self.tok())
            || self.is_ident()
                && (self.s.name_text() == "inline"
                    || self.s.name_text() == "tracked" && self.s.cfg.features.contains(Features::MODULARITY)
                    || matches!(
                        self.s.lookahead().token,
                        Token::ColonOp | Token::ColonFollow | Token::ColonEol
                    ));
        (mods, named)
    }

    fn check_var_args_rules(&mut self, params: &[NodeId]) {
        for (i, &p) in params.iter().enumerate() {
            let Tree::ValDef { tpt, mods, .. } = *self.ast.tree(p) else { continue };
            let star = matches!(*self.ast.tree(tpt), Tree::PostfixOp { op, .. }
                if matches!(self.ast.tree(op), Tree::Ident { name } if self.s.names.text(name.name) == "*"));
            if !star {
                continue;
            }
            let span = self.ast.span(tpt);
            let m = self.ast.modifiers(mods);
            if m.is_one_of(flags::GIVEN_OR_IMPLICIT) {
                let what = if m.is(flags::GIVEN) { "using" } else { "implicit" };
                self.syntax_error_span(
                    format!("repeated parameters are not allowed in a {what} clause"),
                    span,
                );
            }
            if i + 1 < params.len() {
                self.syntax_error_span("varargs parameter must come last", span);
            }
        }
    }
}
