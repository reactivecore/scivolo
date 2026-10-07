use super::ParamOwner;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, Tree};
use crate::parse::{Parser, stage};
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// `id [HkTypeParamClause] {FunParamClause} TypeAndCtxBounds [= Type]` after `type`.
    pub fn type_def_or_dcl(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        self.new_lines_opt();
        let point = self.name_start();
        let name_ident = self.type_ident();
        let capture_var = self.gobble_hat();
        let Tree::Ident { name: tname } = *self.ast.tree(name_ident) else {
            unreachable!("a type identifier")
        };
        let backquoted = self.ast.get(name_ident).attrs.contains(Attrs::BACKQUOTED);
        let tparams = self.type_param_clause_opt(ParamOwner::Hk);
        let mut paramss = vec![tparams];
        while self.tok() == Token::LParen {
            paramss.push(self.fun_param_clause());
        }
        let rhs = match self.tok() {
            Token::Equals => {
                self.next();
                self.type_def_rhs()
            }
            Token::Subtype | Token::Supertype => {
                let bounds = self.type_and_ctx_bounds(tname);
                match *self.ast.tree(bounds) {
                    Tree::TypeBoundsTree { lo, hi, .. } if self.tok() == Token::Equals => {
                        let eq_offset = self.s.skip_token();
                        let rhs = self.type_def_rhs();
                        match *self.ast.tree(rhs) {
                            Tree::MatchTypeTree { selector, cases, .. } => match (lo, hi) {
                                (None, upper) => {
                                    self.ast.node(Tree::MatchTypeTree { bound: upper, selector, cases })
                                }
                                _ => {
                                    self.syntax_error(
                                        "cannot combine lower bound and match type alias",
                                        eq_offset,
                                    );
                                    rhs
                                }
                            },
                            _ if mods.is(flags::OPAQUE) => {
                                self.ast.node(Tree::TypeBoundsTree { lo, hi, alias: Some(rhs) })
                            }
                            _ => {
                                self.syntax_error("cannot combine bound and alias", eq_offset);
                                rhs
                            }
                        }
                    }
                    _ => bounds,
                }
            }
            Token::Semi
            | Token::Newline
            | Token::Newlines
            | Token::Comma
            | Token::RBrace
            | Token::Outdent
            | Token::Eof => self.type_and_ctx_bounds(tname),
            _ if self.staged & stage::QUOTED_PATTERN != 0
                || self.s.cfg.source_version.enables_new_givens()
                    && self.s.cur.is_colon()
                    && !mods.is(flags::OPAQUE) =>
            {
                self.type_and_ctx_bounds(tname)
            }
            _ => {
                let text = format!("=, >:, or <: expected, but {} found", self.tok().display());
                let msg = crate::diag::Message::syntax(95, text);
                self.syntax_error_or_incomplete(msg, self.offset());
                return self.empty_tree();
            }
        };
        let rhs1 = paramss.iter().rev().fold(rhs, |t, ps| self.lambda_abstract(ps, t));
        let m = self.empty_mods();
        let td = self.ast.node(Tree::TypeDef { name: tname, rhs: rhs1, mods: m });
        if backquoted {
            self.ast.add_attrs(td, Attrs::BACKQUOTED);
        }
        if capture_var {
            self.ast.add_attrs(td, Attrs::CAPTURE_VAR);
            self.ast.add_attrs(rhs, Attrs::CAPTURE_VAR);
        }
        let td = self.finalize_def(td, mods);
        self.at_span2(start, point, td)
    }

    /// A type, or a capture set under capture checking.
    fn type_def_rhs(&mut self) -> NodeId {
        self.capture_set_type(false).unwrap_or_else(|| self.toplevel_typ())
    }
}
