// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Lambdas: parameters, closures, and polymorphic function literals.

use super::super::migration::Migration;
use super::super::{Location, Parser};
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, Span, TName, Tree};
use crate::diag::rewrite_notice;
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// `(bindings)`, `id` or `_` before `=>`.
    pub fn fun_params(&mut self, mods: Modifiers, location: Location) -> Vec<NodeId> {
        if self.tok() == Token::LParen {
            self.next();
            if self.tok() == Token::RParen {
                return Vec::new();
            }
            let ps = self.comma_separated(|p| p.binding(mods.clone()));
            self.accept(Token::RParen);
            ps
        } else {
            let start = self.offset();
            let name = self.binding_name();
            let tpt = if matches!(self.tok(), Token::ColonOp | Token::ColonFollow)
                && location == Location::InBlock
            {
                let msg = format!(
                    "This syntax is no longer supported; parameter needs to be enclosed in (...){}",
                    rewrite_notice("This construct", "future-migration", "")
                );
                let span = Span::synthetic(start, self.last_offset());
                self.error_or_migration_warning(msg, span, Migration::ParameterEnclosedByParenthesis);
                self.next();
                self.infix_type()
            } else {
                self.type_tree()
            };
            let p = self.make_parameter(name, tpt, mods, false);
            vec![self.at_span1(start, p)]
        }
    }

    /// `[erased] (id | _) [: Type]`.
    pub fn binding(&mut self, mods: Modifiers) -> NodeId {
        let start = self.offset();
        let mods = if self.is_erased() { self.add_modifier(mods) } else { mods };
        let name = self.binding_name();
        let tpt = self.typed_opt();
        let p = self.make_parameter(name, tpt, mods, false);
        self.at_span1(start, p)
    }

    pub fn binding_name(&mut self) -> TName {
        if self.tok() == Token::Uscore {
            self.next();
            TName { name: self.fresh_name("_$"), is_type: false }
        } else {
            self.ident()
        }
    }

    pub fn closure(&mut self, start: Offset, location: Location, implicit_mods: Modifiers) -> NodeId {
        let params = self.fun_params(implicit_mods, location);
        self.closure_rest(start, location, params)
    }

    /// `=> body` or `?=> body` after the parameters `params`.
    pub fn closure_rest(&mut self, start: Offset, location: Location, params: Vec<NodeId>) -> NodeId {
        let point = self.offset();
        if self.tok() == Token::CtxArrow {
            if params.is_empty() {
                let span = Span::synthetic(start, self.last_offset());
                self.syntax_error_span(
                    "context function literals require at least one formal parameter",
                    span,
                );
            }
            self.next();
        } else {
            self.accept(Token::Arrow);
        }
        let body = if location == Location::InBlock {
            self.block(false)
        } else if location == Location::InColonArg && self.tok() == Token::Indent {
            self.block_expr()
        } else {
            self.expr()
        };
        let args = self.ast.list(&params);
        let f = self.ast.node(Tree::Function { args, body, mods: None, wildcard: false });
        self.at_span2(start, point, f)
    }

    /// `[tparams] => body`, which must be a function (a polymorphic function `kind`).
    pub fn make_poly_function(
        &mut self,
        tparams: Vec<NodeId>,
        body: NodeId,
        kind: &str,
        start: Offset,
        arrow: Offset,
        error_tree: impl FnOnce(&mut Self, Offset) -> NodeId,
    ) -> NodeId {
        let f = self.get_function(body);
        let captures = f.is_some_and(|f| {
            matches!(self.ast.tree(f), Tree::Function { body, .. }
                if matches!(self.ast.tree(*body), Tree::CapturesAndResult { .. }))
        });
        let t = if f.is_none() {
            let msg = format!(
                "Implementation restriction: polymorphic function {kind}s must have a value parameter"
            );
            self.syntax_error(msg, arrow);
            error_tree(self, arrow)
        } else if captures {
            let msg = "Implementation restriction: polymorphic function types cannot wrap function types that have capture sets";
            self.syntax_error(msg, arrow);
            error_tree(self, arrow)
        } else {
            let targs = self.ast.list(&tparams);
            self.ast.node(Tree::PolyFunction { targs, body })
        };
        self.at_span2(start, arrow, t)
    }

    fn get_function(&self, t: NodeId) -> Option<NodeId> {
        match self.ast.tree(t) {
            Tree::Parens { t } => self.get_function(*t),
            Tree::Block { stats, expr: Some(e) } if stats.is_empty() => self.get_function(*e),
            Tree::Function { .. } => Some(t),
            _ => None,
        }
    }
}
