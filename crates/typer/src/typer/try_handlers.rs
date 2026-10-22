// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! A `catch` handler that is an expression, not cases: typed as a function of `Throwable` and
//! applied in one case (dotty's `typedTry(ParsedTry)`, `desugar.makeTryCase`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::defn::type_ref;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::Type;

impl Typer<'_> {
    /// `case ex$ => handler(ex$)` for handler expression `handler` of a `try` expecting `pt`, the
    /// handler a `PartialFunction` applied where defined, `ex$` rethrown elsewhere.
    pub fn handler_case(&mut self, handler: NodeId, pt: Pt, ctx: Ctx) -> Result<NodeId, String> {
        let throwable = self.defn.throwable(self.u)?;
        let throwable = type_ref(self.u, throwable);
        let result = match pt {
            Pt::Type(t) => t,
            _ => self.u.intern_type(Type::Wildcard(None)),
        };
        let function = self.function_class(1).ok_or("scala.Function1")?;
        let function = self.u.intern_type(Type::Applied(function, vec![throwable, result]));
        let typed = self.typed(handler, Pt::Type(function), ctx);
        let whole = self.ast.span(handler).to_synthetic();
        let end = whole.end_pos().to_synthetic();
        let node = |t: &mut Self, tree: Tree, span: Span| {
            let node = t.ast.node(tree);
            t.ast.with_span(node, span);
            node
        };
        let ex = TName { name: self.name("ex$"), is_type: false };
        let ex_ref = |t: &mut Self, span| node(t, Tree::Ident { name: ex }, span);
        let call = |t: &mut Self, qualifier: NodeId, method: &str, span| {
            let name = TName { name: t.name(method), is_type: false };
            let fun = node(t, Tree::Select { qualifier, name }, span);
            let arg = ex_ref(t, end);
            let args = t.ast.list(&[arg]);
            node(t, Tree::Apply { fun, args }, span)
        };
        let widened = self.u.widen(self.tpd.tpe(typed));
        let partial =
            self.u.class_symbol(widened).is_some_and(|c| self.u.full_name(c) == "scala.PartialFunction");
        let handler = self.spliced(typed);
        let rhs = if partial {
            let tmp = TName { name: self.fresh.unique(self.names), is_type: false };
            let tpt = node(self, Tree::TypeTree, whole.start_pos().to_synthetic());
            let mods = self.ast.mods(Modifiers::default());
            let init = node(self, Tree::ValDef { name: tmp, tpt, rhs: Some(handler), mods }, whole);
            let tmp_ref = |t: &mut Self| node(t, Tree::Ident { name: tmp }, end);
            let q = tmp_ref(self);
            let cond = call(self, q, "isDefinedAt", end);
            let q = tmp_ref(self);
            let thenp = call(self, q, "apply", end);
            let expr = ex_ref(self, end);
            let elsep = node(self, Tree::Throw { expr }, end);
            let test = node(self, Tree::If { cond, thenp, elsep: Some(elsep), inline: false }, end);
            let stats = self.ast.list(&[init]);
            node(self, Tree::Block { stats, expr: Some(test) }, whole)
        } else {
            call(self, handler, "apply", whole)
        };
        let pat = ex_ref(self, end);
        Ok(node(self, Tree::CaseDef { pat, guard: None, body: rhs }, whole))
    }
}
