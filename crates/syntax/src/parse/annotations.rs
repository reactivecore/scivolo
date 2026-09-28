use super::Parser;
use crate::ast::mods::Modifiers;
use crate::ast::{NodeId, Tree};
use crate::token::Token;

impl Parser<'_> {
    /// Move the start of annotation or constructor `t` back to `start` (a preceding `@` or
    /// `new`).
    fn adjust_start(&mut self, start: u32, t: NodeId) -> NodeId {
        match *self.ast.tree(t) {
            Tree::Apply { fun, .. } => {
                self.adjust_start(start, fun);
            }
            Tree::Select { qualifier, .. } => {
                self.adjust_start(start, qualifier);
            }
            _ => {}
        }
        let span = self.ast.span(t);
        if span.exists() && start < span.start() {
            self.ast.with_span(t, span.with_start(start));
        }
        t
    }

    /// `@ SimpleType1 {ParArgumentExprs}`.
    pub fn annot(&mut self) -> NodeId {
        let start = self.accept(Token::At);
        let tpt = self.simple_type1();
        let w = self.wrap_new(tpt);
        let a = self.par_argument_exprss(w);
        let a = self.ensure_applied(a);
        self.adjust_start(start, a)
    }

    pub fn annotations(&mut self, skip_new_lines: bool) -> Vec<NodeId> {
        let mut out = Vec::new();
        loop {
            if skip_new_lines {
                self.new_lines_opt_when_followed_by(Token::At);
            }
            if self.tok() != Token::At {
                return out;
            }
            out.push(self.annot());
        }
    }

    pub fn annots_as_mods(&mut self, skip_new_lines: bool) -> Modifiers {
        let annots = self.annotations(skip_new_lines);
        let annotations = self.ast.list(&annots);
        Modifiers { annotations, ..Modifiers::default() }
    }
}
