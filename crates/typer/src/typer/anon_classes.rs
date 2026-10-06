//! Anonymous classes `new P { body }`, typed as `{ final class $anon extends P { body }; new
//! $anon() }` (dotty's `typedNew` of a template).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};

impl Typer<'_> {
    /// `new` of template `template` at `id`, against `pt`.
    pub fn typed_anon_class(&mut self, id: NodeId, template: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        let start = Span::synthetic(span.start(), span.start());
        if let Tree::Template { constr, .. } = *self.ast.tree(template)
            && let Tree::DefDef { paramss, .. } = *self.ast.tree(constr)
            && self.ast.clause_items(paramss).is_empty()
        {
            let empty = self.ast.list(&[]);
            let clauses = self.ast.clauses(&[empty]);
            if let Tree::DefDef { paramss, .. } = &mut self.ast.get_mut(constr).tree {
                *paramss = clauses;
            }
        }
        let name = TName { name: self.name("$anon"), is_type: true };
        let mods = self.ast.mods(Modifiers::with_flags(f::FINAL.or(f::SYNTHETIC)));
        let class = self.ast.node(Tree::TypeDef { name, rhs: template, mods });
        let template_span = self.ast.span(template);
        self.ast.with_span(class, Span::synthetic(template_span.start(), template_span.end()));
        let ident = self.ast.node(Tree::Ident { name });
        self.ast.with_span(ident, start);
        let new = self.ast.node(Tree::New { tpt: ident });
        self.ast.with_span(new, start);
        let init = TName { name: self.name("<init>"), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: new, name: init });
        self.ast.with_span(select, start);
        let args = self.ast.list(&[]);
        let apply = self.ast.node(Tree::Apply { fun: select, args });
        self.ast.with_span(apply, span);
        let stats = self.ast.list(&[class]);
        let block = self.ast.node(Tree::Block { stats, expr: Some(apply) });
        self.ast.with_span(block, span);
        self.typed_block(block, stats, Some(apply), pt, ctx)
    }
}
