use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, Span, TName, Tree};
use crate::diag::rewrite_notice;
use crate::parse::migration::Migration;
use crate::parse::{Location, Parser};
use crate::source::Offset;
use crate::token::Token;
use crate::token::sets::KEYWORDS;

impl Parser<'_> {
    /// `ids [: Type] [= Expr]` or `Pattern2 [: Type] [= Expr]` after `val` or `var`.
    pub fn pat_def_or_dcl(&mut self, start: Offset, mods: Modifiers) -> NodeId {
        let point = self.name_start();
        if self.tok() != Token::Uscore && KEYWORDS.contains(self.tok()) {
            let msg = Self::expected_but_found(Token::Identifier, self.tok(), "", "");
            let o = self.offset();
            self.syntax_error_span(msg, Span::at(o));
        }
        let first = self.pattern2(Location::InPattern);
        let mut lhs = vec![first];
        if matches!(self.ast.tree(first), Tree::Ident { .. }) && self.tok() == Token::Comma {
            self.next();
            lhs.extend(self.comma_separated(|p| p.term_ident()));
        }
        let at_colon = self.s.cur.is_colon();
        let tpt = self.typed_opt();
        if at_colon {
            self.check_ascription(first, tpt, false);
        }
        let tpt_empty = self.is_empty(tpt);
        let rhs = if tpt_empty || self.tok() == Token::Equals {
            let help = match lhs[..] {
                [id] if tpt_empty && self.tok() != Token::Equals => self.colon_name_help(id),
                _ => String::new(),
            };
            self.accept_help(Token::Equals, &help);
            let rhs_offset = self.offset();
            let rhs = self.sub_expr();
            Some(self.uninitialized_rhs(rhs, rhs_offset, tpt_empty, &mods, &lhs))
        } else {
            None
        };
        let t = match lhs[..] {
            [id] if self.id_pattern(id).is_some_and(|(_, t)| t.is_none_or(|t| self.is_empty(t))) => {
                let (id, _) = self.id_pattern(id).expect("an identifier pattern");
                let Tree::Ident { name } = *self.ast.tree(id) else { unreachable!("an identifier") };
                let m = self.empty_mods();
                let name = TName { is_type: false, ..name };
                let v = self.ast.node(Tree::ValDef { name, tpt, rhs, mods: m });
                if self.ast.get(id).attrs.contains(Attrs::BACKQUOTED) {
                    self.ast.add_attrs(v, Attrs::BACKQUOTED);
                }
                self.finalize_def(v, mods)
            }
            _ => {
                let all_ids = lhs
                    .iter()
                    .all(|&p| self.id_pattern(p).is_some_and(|(_, t)| t.is_none_or(|t| self.is_empty(t))));
                let rhs = if rhs.is_none() && !all_ids {
                    let s = self.last_offset();
                    let msg = Self::expected_but_found(Token::Equals, self.tok(), "", "");
                    self.syntax_error_span(msg, Span::at(s));
                    Some(self.error_term_tree(s))
                } else {
                    rhs
                };
                let pats = self.ast.list(&lhs);
                let m = self.ast.mods(mods);
                self.ast.node(Tree::PatDef { mods: m, pats, tpt, rhs })
            }
        };
        self.at_span2(start, point, t)
    }

    /// The hint for a missing `=` after identifier `id` ending in `:`.
    pub fn colon_name_help(&self, id: NodeId) -> String {
        let Tree::Ident { name } = *self.ast.tree(id) else { return String::new() };
        let text = self.s.names.text(name.name);
        match text.strip_suffix(':') {
            Some(stem) => format!("; identifier ends in colon, did you mean `{stem}`: in backticks?"),
            None => String::new(),
        }
    }

    /// An identifier `x` (not `_`) with no type, or `x: T` (dotty's `IdPattern`).
    fn id_pattern(&self, p: NodeId) -> Option<(NodeId, Option<NodeId>)> {
        match *self.ast.tree(p) {
            Tree::Ident { name } if self.s.names.text(name.name) != "_" => Some((p, None)),
            Tree::Typed { expr, tpt } if matches!(self.ast.tree(expr), Tree::Ident { .. }) => {
                Some((expr, Some(tpt)))
            }
            _ => None,
        }
    }

    /// `var x: T = _`, deprecated: the rhs becomes `_`.
    fn uninitialized_rhs(
        &mut self,
        rhs: NodeId,
        rhs_offset: Offset,
        tpt_empty: bool,
        mods: &Modifiers,
        lhs: &[NodeId],
    ) -> NodeId {
        let placeholder = match *self.ast.tree(rhs) {
            Tree::Ident { name } => self
                .placeholder_params
                .last()
                .is_some_and(|&p| matches!(self.ast.tree(p), Tree::ValDef { name: pn, .. } if *pn == name)),
            _ => false,
        };
        let all_idents = lhs.iter().all(|&p| matches!(self.ast.tree(p), Tree::Ident { .. }));
        if !(placeholder && !tpt_empty && mods.is(flags::MUTABLE) && all_idents) {
            return rhs;
        }
        let msg = format!(
            "`= _` has been deprecated; use `= uninitialized` instead.\n`uninitialized` can be imported with `scala.compiletime.uninitialized`.{}",
            rewrite_notice("This construct", "3.4-migration", "")
        );
        self.migration_error(msg, rhs_offset, Migration::UninitializedVars);
        self.placeholder_params.pop();
        let span = self.ast.span(rhs);
        let name = self.term("_");
        let w = self.ast.node(Tree::Ident { name });
        self.at_span(span, w)
    }
}
