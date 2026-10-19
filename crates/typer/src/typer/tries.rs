//! `try` expressions (dotty's `typedTry`): the handler's cases, a finalizer, and the least upper
//! bound of the expression and the cases as the type.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::tree::MatchKind;
use syntax::ast::{NodeId, Tree};
use tasty::{NOTHING, TypeId};

impl Typer<'_> {
    /// `try expr catch handler finally finalizer` as parsed, its handler cases or none.
    pub fn typed_parsed_try(
        &mut self,
        id: NodeId,
        expr: NodeId,
        handler: Option<NodeId>,
        finalizer: Option<NodeId>,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let cases = match handler.map(|h| self.ast.tree(h).clone()) {
            None => Vec::new(),
            Some(Tree::Match { selector: None, cases, kind: MatchKind::Plain }) => {
                self.ast.items(cases).to_vec()
            }
            Some(_) => return self.unsupported(id, "handler expression"),
        };
        self.typed_try(id, expr, &cases, finalizer, pt, ctx)
    }

    pub fn typed_try(
        &mut self,
        id: NodeId,
        expr: NodeId,
        cases: &[NodeId],
        finalizer: Option<NodeId>,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let pt = match pt {
            Pt::Fun(_) | Pt::PolyFun(_) => Pt::Any,
            pt => pt,
        };
        let expr = self.typed(expr, pt, ctx);
        let throwable = match self.defn.throwable(self.u) {
            Ok(t) => crate::defn::type_ref(self.u, t),
            Err(e) => return self.unsupported(id, &e),
        };
        let typed_cases: Vec<TNodeId> =
            cases.iter().map(|&c| self.typed_case(c, throwable, pt, ctx)).collect();
        let mut types: Vec<TypeId> = vec![self.by_name_result(self.tpd.tpe(expr))];
        for &c in &typed_cases {
            let T::CaseDef { body, .. } = *self.tpd.tree(c) else { return self.unsupported(id, "case") };
            types.push(self.by_name_result(self.tpd.tpe(body)));
        }
        if matches!(pt, Pt::Any) && types.windows(2).any(|w| self.needs_harmonizing(w[0], w[1])) {
            return self.unsupported(id, "numeric harmonization");
        }
        let tpe = types.iter().fold(NOTHING, |acc, &t| self.u.lub(acc, t));
        let unit = crate::defn::type_ref(self.u, self.defn.unit);
        let finalizer = finalizer.map(|f| self.typed(f, Pt::Type(unit), ctx));
        let cases = self.tpd.list(&typed_cases);
        let span = self.ast.span(id);
        self.tpd.node(T::Try { expr, cases, finalizer }, span, Some(tpe), None)
    }
}
