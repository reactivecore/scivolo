//! The signature of a given: its type parameters, parameter clauses and parents.

use super::ParamOwner;
use crate::ast::flags;
use crate::ast::mods::Modifiers;
use crate::ast::{List, NodeId, Tree};
use crate::parse::Parser;
use crate::token::Token;

type Signature = (Vec<NodeId>, Vec<List>, Vec<NodeId>);

impl Parser<'_> {
    /// `[tparams =>] {(params) =>} parents` of a new-style given.
    pub fn new_signature(&mut self, new_syntax: bool) -> Signature {
        let tparams = if self.tok() == Token::LBracket {
            let t = self.type_param_clause(ParamOwner::Given);
            self.accept(Token::Arrow);
            t
        } else {
            Vec::new()
        };
        let (paramss, parents) = self.given_params_and_parents(0, new_syntax);
        (tparams, paramss, parents)
    }

    fn given_params_and_parents(&mut self, num_lead: usize, new_syntax: bool) -> (Vec<List>, Vec<NodeId>) {
        if self.tok() == Token::LParen && self.following_is_arrow() {
            let params = if self.s.lookahead().token == Token::RParen && num_lead == 0 {
                self.next();
                self.next();
                Vec::new()
            } else {
                self.term_param_clause(ParamOwner::Given, num_lead, true, Modifiers::with_flags(flags::GIVEN))
            };
            self.accept(Token::Arrow);
            let clause = self.ast.list(&params);
            if params.is_empty() {
                return (vec![clause], self.implemented(new_syntax));
            }
            let (mut paramss, parents) = self.given_params_and_parents(num_lead + params.len(), new_syntax);
            paramss.insert(0, clause);
            return (paramss, parents);
        }
        let parents = self.implemented(new_syntax);
        if self.tok() == Token::Arrow && parents.len() == 1 && self.is_type(parents[0]) {
            self.next();
            let (mut paramss, parents1) = self.given_params_and_parents(num_lead + 1, new_syntax);
            let ps = self.types_to_params(&parents, ParamOwner::Given, num_lead, flags::GIVEN);
            paramss.insert(0, self.ast.list(&ps));
            return (paramss, parents1);
        }
        (Vec::new(), parents)
    }

    /// The parents or the type a given implements.
    pub fn implemented(&mut self, new_syntax: bool) -> Vec<NodeId> {
        if self.is_simple_literal() {
            let t = self.annot_type();
            return vec![self.reject_wildcard_type(t)];
        }
        let parent = self.constr_app();
        if !matches!(self.ast.tree(parent), Tree::Apply { .. }) && self.is_ident() && new_syntax {
            return vec![self.infix_type_rest_with(parent, false, |p, _| p.annot_type1())];
        }
        let mut out = vec![parent];
        if new_syntax && self.tok() == Token::Comma {
            self.next();
            out.extend(self.constr_apps(None));
        } else {
            out.extend(self.with_constr_apps());
        }
        out
    }

    pub fn is_simple_literal(&mut self) -> bool {
        crate::token::sets::SIMPLE_LITERAL_TOKENS.contains(self.tok()) || self.is_negated_number()
    }
}
