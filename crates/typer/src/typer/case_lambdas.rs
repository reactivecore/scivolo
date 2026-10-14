//! Case lambdas `{ case p => e }` (dotty's `makeCaseLambda`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::flags;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::tree::MatchKind;
use syntax::ast::{List, NodeId, Span, TName, Tree};

impl Typer<'_> {
    /// `{ cases }` as `(x$1, ..., x$n) => (x$1, ..., x$n) match { cases }`, `n` the arity of the
    /// function type `pt` or 1 (dotty's `makeCaseLambda`, `decomposeProtoFunction`).
    pub fn typed_case_lambda(&mut self, id: NodeId, cases: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let items = self.ast.items(cases).to_vec();
        let (Some(&first), Some(&last)) = (items.first(), items.last()) else {
            return self.unsupported(id, "case lambda without cases");
        };
        let (start, end) = (self.ast.span(first).start(), self.ast.span(last).end());
        let at = Span::synthetic(start, start);
        let n = match pt {
            Pt::Type(p) => (1..=22).find(|&n| self.function_parts(p, n).is_some()).unwrap_or(1),
            _ => 1,
        };
        let node = |t: &mut Self, tree| {
            let id = t.ast.node(tree);
            t.ast.with_span(id, at);
            id
        };
        let (mut params, mut refs) = (Vec::new(), Vec::new());
        for i in 1..=n {
            let name = TName { name: self.name(&format!("x${i}")), is_type: false };
            let tpt = self.ast.node(Tree::TypeTree);
            let mods = self.ast.mods(Modifiers::with_flags(flags::SYNTHETIC_TERM_PARAM));
            params.push(node(self, Tree::ValDef { name, tpt, rhs: None, mods }));
            refs.push(node(self, Tree::Ident { name }));
        }
        let selector = match refs[..] {
            [r] => r,
            _ => {
                let sym = TName { name: self.name(&format!("scala.Tuple{n}")), is_type: false };
                let fun = node(self, Tree::TypedSplice { sym });
                let args = self.ast.list(&refs);
                node(self, Tree::Apply { fun, args })
            }
        };
        let body = self.ast.node(Tree::Match { selector: Some(selector), cases, kind: MatchKind::Plain });
        self.ast.with_span(body, Span::synthetic(start, end));
        let args = self.ast.list(&params);
        self.typed_function(id, args, body, pt, ctx)
    }
}
