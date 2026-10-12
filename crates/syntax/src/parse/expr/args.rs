//! Arguments and blocks.

use super::super::{Location, Parser};
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, Tree};
use crate::token::Token;

/// Arguments and whether they were a `using` clause.
pub type Args = (Vec<NodeId>, bool);

impl Parser<'_> {
    /// The contents of `(...)`: expressions, named expressions, or lambda parameters.
    pub fn exprs_in_parens_or_bindings(&mut self) -> Vec<NodeId> {
        if self.tok() == Token::RParen {
            return Vec::new();
        }
        let r = self.s.region;
        self.s.regions.get_mut(r).commas_expected = true;
        let mut formal = false;
        let mut part = |p: &mut Self| {
            if p.is_erased() {
                formal = true;
            }
            if formal {
                p.binding(Modifiers::default())
            } else {
                let t = p.maybe_named(|p| p.expr_in_parens());
                if matches!(p.ast.tree(t), Tree::ValDef { .. }) {
                    formal = true;
                }
                t
            }
        };
        let first = part(self);
        let ts = self.comma_separated_rest(first, part);
        self.s.regions.get_mut(r).commas_expected = false;
        ts
    }

    /// `( [using] [exprs] )`.
    pub fn par_argument_exprs(&mut self) -> Args {
        self.in_parens_with_commas(|p| {
            if p.tok() == Token::RParen {
                (Vec::new(), false)
            } else if p.is_ident_named("using") {
                p.next();
                (p.comma_separated(Self::argument_expr), true)
            } else {
                (p.comma_separated(Self::argument_expr), false)
            }
        })
    }

    /// `ParArgumentExprs` or a block argument.
    pub fn argument_exprs(&mut self) -> Args {
        if matches!(self.tok(), Token::LBrace | Token::Indent) {
            (vec![self.block_expr()], false)
        } else {
            self.par_argument_exprs()
        }
    }

    /// `fun` applied to `ArgumentExprs`.
    pub fn argument_apply(&mut self, fun: NodeId) -> NodeId {
        let parens = self.tok() == Token::LParen;
        let args = self.argument_exprs();
        let app = self.mk_apply(fun, args);
        if parens {
            self.ast.add_attrs(app, Attrs::PAREN_ARGS);
        }
        app
    }

    pub fn mk_apply(&mut self, fun: NodeId, (args, using): Args) -> NodeId {
        let args = self.ast.list(&args);
        let app = self.ast.node(Tree::Apply { fun, args });
        if using {
            self.ast.add_attrs(app, Attrs::USING_APPLY);
        }
        app
    }

    /// An argument; `x = e` is a named argument.
    pub fn argument_expr(&mut self) -> NodeId {
        let arg = self.expr_at(Location::InArgs);
        if let Tree::Assign { lhs, rhs } = *self.ast.tree(arg)
            && let Tree::Ident { name } = *self.ast.tree(lhs)
        {
            self.ast.get_mut(arg).tree = Tree::NamedArg { name, arg: rhs };
        }
        arg
    }

    /// `{ArgumentExprs}` after `fun`.
    pub fn argument_exprss(&mut self, mut fun: NodeId) -> NodeId {
        loop {
            self.argument_start();
            if !matches!(self.tok(), Token::LParen | Token::LBrace | Token::Indent) {
                return fun;
            }
            fun = self.argument_apply(fun);
        }
    }

    /// `{ParArgumentExprs}` after `fun`; in constructor annotations, `(...)` that looks like a
    /// parameter list is not taken as arguments.
    pub fn par_argument_exprss(&mut self, mut fun: NodeId) -> NodeId {
        while self.tok() == Token::LParen && (!self.in_class_constr_annots || self.is_legal_annot_arg(fun)) {
            let start = self.start_offset(fun);
            let args = self.par_argument_exprs();
            let app = self.mk_apply(fun, args);
            self.ast.add_attrs(app, Attrs::PAREN_ARGS);
            fun = self.at_span1(start, app);
        }
        fun
    }

    fn is_legal_annot_arg(&mut self, fun: NodeId) -> bool {
        let applied = matches!(self.ast.tree(fun), Tree::Apply { .. });
        let exprs = self.s.can_start_expr_tokens();
        let mut la = self.s.lookahead_scanner();
        la.cur.token == Token::LParen && {
            la.next_token();
            match la.cur.token {
                Token::RParen => !applied,
                Token::Identifier => {
                    la.next_token();
                    !matches!(la.cur.token, Token::ColonOp | Token::ColonFollow | Token::ColonEol)
                }
                t => exprs.contains(t),
            }
        }
    }

    /// `{ CaseClauses }`, `{ Block }`, or their indented forms.
    pub fn block_expr(&mut self) -> NodeId {
        let start = self.offset();
        let simplify = self.tok() == Token::Indent;
        let t = self.in_braces_or_indented(|p| {
            if p.tok() == Token::Case {
                let cases = p.case_clauses(|p| p.case_clause(false));
                let cases = p.ast.list(&cases);
                p.ast.node(Tree::Match { selector: None, cases, kind: crate::ast::tree::MatchKind::Plain })
            } else {
                p.block(simplify)
            }
        });
        self.at_span1(start, t)
    }

    /// A block's statements; the tree has no span yet.
    pub fn block(&mut self, simplify: bool) -> NodeId {
        let stats = self.block_stat_seq();
        let is_expr =
            |p: &Self, s: NodeId| !(p.ast.is_def(s) || matches!(p.ast.tree(s), Tree::Import { .. }));
        if let Some((&last, inits)) = stats.split_last()
            && is_expr(self, last)
        {
            if inits.is_empty() && (simplify || matches!(self.ast.tree(last), Tree::Block { .. })) {
                return last;
            }
            let inits = self.ast.list(inits);
            return self.ast.node(Tree::Block { stats: inits, expr: Some(last) });
        }
        let stats = self.ast.list(&stats);
        self.ast.node(Tree::Block { stats, expr: None })
    }
}
