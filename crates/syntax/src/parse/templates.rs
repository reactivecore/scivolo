//! Templates: parents, `derives` clauses and template bodies.

use super::Parser;
use super::migration::Migration;
use crate::ast::{NodeId, Span, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// `AnnotType1 {ParArgumentExprs}`.
    pub fn constr_app(&mut self) -> NodeId {
        let t = self.annot_type1();
        let t = if self.is_wildcard_type(t) {
            let span = self.ast.span(t);
            self.syntax_error_span("Unbound wildcard type", span);
            let error = self.type_name("<error>");
            self.ast.node(Tree::Ident { name: error })
        } else {
            t
        };
        if self.tok() == Token::LParen {
            let w = self.wrap_new(t);
            self.par_argument_exprss(w)
        } else {
            t
        }
    }

    /// `ConstrApp ({, ConstrApp} | {with ConstrApp})`.
    pub fn constr_apps(&mut self, exclude: Option<Token>) -> Vec<NodeId> {
        let mut out = vec![self.constr_app()];
        let mut exclude = exclude;
        loop {
            let tok = self.tok();
            if !matches!(tok, Token::With | Token::Comma) || exclude == Some(tok) {
                return out;
            }
            self.next();
            exclude = Some(if tok == Token::With { Token::Comma } else { Token::With });
            out.push(self.constr_app());
        }
    }

    /// `{with ConstrApp}` with no line end or `{` after `with`.
    pub fn with_constr_apps(&mut self) -> Vec<NodeId> {
        let mut out = Vec::new();
        while self.tok() == Token::With {
            let la = self.s.lookahead();
            if la.is_after_line_end() || la.token == Token::LBrace {
                break;
            }
            self.next();
            out.push(self.constr_app());
        }
        out
    }

    /// `[extends ConstrApps] [derives QualId {, QualId}] [TemplateBody]`.
    pub fn template(&mut self, constr: NodeId, is_enum: bool) -> NodeId {
        let parents = if self.tok() == Token::Extends {
            self.next();
            if matches!(self.tok(), Token::LBrace | Token::ColonEol) {
                self.migration_error(
                    "`extends` must be followed by at least one parent",
                    self.offset(),
                    Migration::Scala2to3,
                );
                Vec::new()
            } else {
                self.constr_apps(None)
            }
        } else {
            Vec::new()
        };
        self.new_lines_opt_when_followed_by_name("derives");
        let derived = if self.is_ident_named("derives") {
            self.next();
            self.comma_separated(|p| {
                let q = p.qual_id();
                p.convert_to_type_id(q)
            })
        } else {
            Vec::new()
        };
        self.new_template_start();
        if is_enum {
            let saved = std::mem::replace(&mut self.in_enum, true);
            let (self_, stats) = self.template_body(&parents);
            self.in_enum = saved;
            self.make_template(constr, &parents, &derived, self_, &stats)
        } else {
            self.template_body_opt(constr, &parents, &derived)
        }
    }

    pub fn template_opt(&mut self, constr: NodeId) -> NodeId {
        self.new_lines_opt_when_followed_by_name("derives");
        if self.tok() == Token::Extends || self.is_ident_named("derives") {
            return self.template(constr, false);
        }
        self.new_template_start();
        if self.s.cur.is_nested_start() {
            self.template(constr, false)
        } else {
            self.check_next_not_indented();
            self.make_template(constr, &[], &[], None, &[])
        }
    }

    pub fn template_body_opt(&mut self, constr: NodeId, parents: &[NodeId], derived: &[NodeId]) -> NodeId {
        let (self_, stats) = if self.s.cur.is_nested_start() {
            self.template_body(parents)
        } else {
            self.check_next_not_indented();
            (None, Vec::new())
        };
        self.make_template(constr, parents, derived, self_, &stats)
    }

    /// `{ TemplateStatSeq }` or an indented `TemplateStatSeq`.
    pub fn template_body(&mut self, parents: &[NodeId]) -> (Option<NodeId>, Vec<NodeId>) {
        let r = self.in_braces_or_indented(|p| p.template_stat_seq());
        if self.tok() == Token::With && parents.is_empty() {
            let o = self.offset();
            self.syntax_error("Early definitions are not supported; use trait parameters instead", o);
            self.next();
            let c = self.empty_constructor();
            self.template(c, false);
        }
        r
    }

    /// `with TemplateBody` of an old-style given.
    pub fn with_template(&mut self, constr: NodeId, parents: &[NodeId]) -> NodeId {
        let msg = "Given member definitions starting with `with` are no longer supported; use `{...}` or `:` followed by newline instead";
        self.migration_error(msg, self.offset(), Migration::GivenSyntax);
        self.accept(Token::With);
        let (self_, stats) = self.template_body(parents);
        let t = self.make_template(constr, parents, &[], self_, &stats);
        let cs = self.ast.span(constr);
        let start = if cs.exists() { cs.start() } else { self.ast.span(parents[0]).start() };
        self.ast.with_span(t, Span::synthetic(start, self.last_offset()));
        t
    }

    /// dotty's `untpd.Template`, a `DerivingTemplate` if `derived` is not empty.
    pub fn make_template(
        &mut self,
        constr: NodeId,
        parents: &[NodeId],
        derived: &[NodeId],
        self_: Option<NodeId>,
        body: &[NodeId],
    ) -> NodeId {
        let all: Vec<NodeId> = parents.iter().chain(derived).copied().collect();
        let parents_or_derived = self.ast.list(&all);
        let body = self.ast.list(body);
        let derived = (!derived.is_empty()).then_some(derived.len() as u32);
        self.ast.node(Tree::Template { constr, parents_or_derived, self_, body, derived })
    }

    /// Warn if the next line is indented without a `{` or `:` opening a body.
    pub fn check_next_not_indented(&mut self) {
        if !self.s.cur.is_new_line() {
            return;
        }
        let next = self.s.next.offset;
        let width = self.s.indent_width(next);
        let r = self.s.region;
        if self.s.regions.indent_width(r).lt(&width) && self.s.regions.closed_by(r) == Token::Outdent {
            let msg = format!(
                "Line is indented too far to the right, or a {} or {} is missing",
                Token::LBrace.display(),
                Token::ColonOp.display()
            );
            self.warning(msg, next);
        }
    }
}
