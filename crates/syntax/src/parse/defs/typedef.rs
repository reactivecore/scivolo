use super::ParamOwner;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, Tree};
use crate::config::Features;
use crate::parse::{Parser, stage};
use crate::source::Offset;
use crate::token::Token;

impl Parser<'_> {
    /// `id [HkTypeParamClause] {FunParamClause} TypeAndCtxBounds [= Type]` after `type`.
    pub fn type_def_or_dcl(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        self.new_lines_opt();
        let point = self.name_start();
        let name_ident = self.type_ident();
        self.gobble_hat();
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
                self.toplevel_typ()
            }
            Token::Subtype | Token::Supertype => {
                let bounds = self.type_and_ctx_bounds(tname);
                match *self.ast.tree(bounds) {
                    Tree::TypeBoundsTree { lo, hi, .. } if self.tok() == Token::Equals => {
                        let eq_offset = self.s.skip_token();
                        let rhs = self.toplevel_typ();
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
                let msg = format!("=, >:, or <: expected, but {} found", self.tok().display());
                self.syntax_error_or_incomplete(msg, self.offset());
                return self.empty_tree();
            }
        };
        let rhs = paramss.iter().rev().fold(rhs, |t, ps| self.lambda_abstract(ps, t));
        let m = self.empty_mods();
        let td = self.ast.node(Tree::TypeDef { name: tname, rhs, mods: m });
        if backquoted {
            self.ast.add_attrs(td, Attrs::BACKQUOTED);
        }
        let td = self.finalize_def(td, mods);
        self.at_span2(start, point, td)
    }

    /// Skip `^` after a name under capture checking.
    pub fn gobble_hat(&mut self) -> bool {
        let hat = self.s.cfg.features.contains(Features::CAPTURE_CHECKING) && self.is_ident_named("^");
        if hat {
            self.next();
        }
        hat
    }
}
