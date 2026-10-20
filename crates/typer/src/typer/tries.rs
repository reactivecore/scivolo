// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
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
        let throwable = match self.defn.throwable(self.u) {
            Ok(t) => crate::defn::type_ref(self.u, t),
            Err(e) => return self.unsupported(id, &e),
        };
        let before = self.before_harmonic();
        let expr = self.typed(expr, pt, ctx);
        let mut all = vec![expr];
        all.extend(cases.iter().map(|&c| self.typed_case(c, throwable, pt, ctx)));
        let all = match self.harmonic(all, pt, Some(before), ctx) {
            Ok(h) => h,
            Err(what) => return self.unsupported(id, what),
        };
        let (expr, typed_cases) = (all[0], &all[1..]);
        let mut types: Vec<TypeId> = vec![self.by_name_result(self.tpd.tpe(expr))];
        for &c in typed_cases {
            let T::CaseDef { body, .. } = *self.tpd.tree(c) else { return self.unsupported(id, "case") };
            types.push(self.by_name_result(self.tpd.tpe(body)));
        }
        let tpe = types.iter().fold(NOTHING, |acc, &t| self.u.lub(acc, t));
        let unit = crate::defn::type_ref(self.u, self.defn.unit);
        let finalizer = finalizer.map(|f| self.typed(f, Pt::Type(unit), ctx));
        let cases = self.tpd.list(typed_cases);
        let span = self.ast.span(id);
        self.tpd.node(T::Try { expr, cases, finalizer }, span, Some(tpe), None)
    }
}
