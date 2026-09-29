use super::super::Parser;
use crate::ast::{NodeId, Span, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// `new ConstrApp {with ConstrApp} [TemplateBody]` or `new TemplateBody`.
    pub fn new_expr(&mut self) -> NodeId {
        let start = self.s.skip_token();
        self.new_template_start();
        let parents = if matches!(self.tok(), Token::LBrace | Token::Indent) {
            Vec::new()
        } else {
            self.constr_apps(Some(Token::Comma))
        };
        let colonized = self.new_template_start();
        let nested = matches!(self.tok(), Token::LBrace | Token::Indent);
        if let [parent] = parents[..]
            && !nested
        {
            let t = if colonized {
                let constr = self.empty_constructor();
                let parents = self.ast.list(&parents);
                let body = self.ast.list(&[]);
                let tmpl = self.ast.node(Tree::Template {
                    constr,
                    parents_or_derived: parents,
                    self_: None,
                    body,
                    derived: None,
                });
                self.ast.node(Tree::New { tpt: tmpl })
            } else if self.ast.is_type(parent) {
                let w = self.wrap_new(parent);
                self.ensure_applied(w)
            } else {
                parent
            };
            return self.reposition(t, start);
        }
        let indented = self.tok() == Token::Indent;
        let constr = self.empty_constructor();
        let tbo = self.template_body_opt(constr, &parents, &[]);
        let body = if indented {
            let span = self.ast.span(tbo).with_start(start);
            self.ast.with_span(tbo, span);
            tbo
        } else {
            self.reposition(tbo, start)
        };
        self.ast.node(Tree::New { tpt: body })
    }

    fn reposition(&mut self, t: NodeId, start: u32) -> NodeId {
        let span = self.ast.span(t);
        let end = if span.exists() { self.last_offset().max(span.end()) } else { self.last_offset() };
        self.ast.with_span(t, Span::synthetic(start, end));
        t
    }
}
