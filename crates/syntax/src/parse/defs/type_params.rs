use super::super::Parser;
use super::ParamOwner;
use crate::ast::flags;
use crate::ast::{NodeId, TName, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// `[TypeParam {, TypeParam}]`.
    pub fn type_param_clause(&mut self, owner: ParamOwner) -> Vec<NodeId> {
        self.in_brackets_with_commas(|p| p.comma_separated(|p| p.type_param(owner)))
    }

    pub fn type_param_clause_opt(&mut self, owner: ParamOwner) -> Vec<NodeId> {
        if self.tok() == Token::LBracket { self.type_param_clause(owner) } else { Vec::new() }
    }

    fn type_param(&mut self, owner: ParamOwner) -> NodeId {
        let start = self.offset();
        let mut mods = self.annots_as_mods(false);
        mods.flags = mods.flags.or(flags::PARAM);
        if owner.is_class() {
            mods.flags = mods.flags.or(flags::PRIVATE_LOCAL);
        }
        if self.is_ident_named("+") && self.check_variance_ok(owner) {
            mods.flags = mods.flags.or(flags::COVARIANT);
        } else if self.is_ident_named("-") && self.check_variance_ok(owner) {
            mods.flags = mods.flags.or(flags::CONTRAVARIANT);
        }
        let point = self.name_start();
        let name = if owner.accepts_wildcard() && self.tok() == Token::Uscore {
            self.next();
            TName { name: self.fresh_name("_$"), is_type: true }
        } else {
            TName { is_type: true, ..self.ident() }
        };
        self.gobble_hat();
        let hk = self.type_param_clause_opt(ParamOwner::Hk);
        let bounds = self.type_and_ctx_bounds(name);
        let bounds = match *self.ast.tree(bounds) {
            Tree::ContextBounds { bounds: b, cx_bounds }
                if !owner.accepts_ctx_bounds(self.s.cfg.source_version) =>
            {
                for &cb in self.ast.items(cx_bounds).to_vec().iter() {
                    let span = self.ast.span(cb);
                    self.syntax_error_span("Context bounds are not allowed in this position", span);
                }
                b
            }
            _ => bounds,
        };
        let rhs = self.lambda_abstract(&hk, bounds);
        let mods = self.ast.mods(mods);
        let t = self.ast.node(Tree::TypeDef { name, rhs, mods });
        self.at_span2(start, point, t)
    }

    fn check_variance_ok(&mut self, owner: ParamOwner) -> bool {
        let ok = owner.accepts_variance();
        if !ok {
            let o = self.offset();
            self.syntax_error("no `+/-` variance annotation allowed here", o);
        }
        self.next();
        ok
    }

    /// `[tparams] =>> t` for higher-kinded parameters (dotty's `lambdaAbstract`).
    pub fn lambda_abstract(&mut self, params: &[NodeId], t: NodeId) -> NodeId {
        let Some(&first) = params.first() else { return t };
        let list = self.ast.list(params);
        if matches!(self.ast.tree(first), Tree::ValDef { .. }) {
            self.ast.node(Tree::TermLambdaTypeTree { params: list, body: t })
        } else {
            self.ast.node(Tree::LambdaTypeTree { tparams: list, body: t })
        }
    }
}
