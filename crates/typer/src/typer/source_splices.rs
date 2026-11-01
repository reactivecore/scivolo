// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/QuotesAndSplices.scala
//! Term quotes and splices as dotty types them: `'{ e }` as `scala.quoted.runtime.Expr.quote(e)`
//! applied to a summoned `Quotes`, `${ e }` as `scala.quoted.runtime.Expr.splice(e)`.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree, flags as f};
use tasty::Type;

impl Typer<'_> {
    /// `${ expr }`: at level 0 a top-level splice of an inline method, which becomes a macro.
    pub fn typed_splice(&mut self, id: NodeId, expr: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if let Tree::Quote { body } = *self.ast.tree(expr)
            && !self.ast.is_type(body)
        {
            let msg = "Canceled quote directly inside a splice. ${ '{ XYZ } } is equivalent to XYZ.";
            self.warning_at(self.ast.span(id), msg);
            return self.typed(body, pt, ctx);
        }
        if ctx.staging < 0 {
            return self.unsupported(id, "Splice");
        }
        if ctx.staging == 0 {
            let Some(meth) = self.enclosing_inline(ctx.owner) else { return self.unsupported(id, "Splice") };
            let flags = self.syms.flags[&meth];
            self.syms.set_flags(self.u, meth, flags.or(f::MACRO));
        }
        let span = self.ast.span(id);
        self.runtime_call(span, "splice", expr, pt, Ctx { staging: ctx.staging - 1, ..ctx })
    }

    /// `'{ body }`, a term quote.
    pub fn typed_quote(&mut self, id: NodeId, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if ctx.mode.is(Mode::PATTERN) {
            return self.typed_quote_pattern(id, body, pt, ctx);
        }
        if self.ast.is_type(body) {
            return self.unsupported(id, "Quote");
        }
        if matches!(self.ast.tree(body), Tree::Splice { .. }) {
            let msg = "Canceled splice directly inside a quote. '{ ${ XYZ } } is equivalent to XYZ.";
            self.warning_at(self.ast.span(id), msg);
        }
        let span = self.ast.span(body).to_synthetic();
        let applied = self.runtime_call(span, "quote", body, pt, Ctx { staging: ctx.staging + 1, ..ctx });
        self.as_quote(applied, self.ast.span(id));
        applied
    }

    /// Gives `quote(e).apply(quotes)`, `applied`, span `span`, `quotes` the start of `span` and
    /// `quote(e)` dotty's type of a `Quote`, `ContextFunction1[Quotes, Expr[T]]`.
    fn as_quote(&mut self, applied: TNodeId, span: Span) {
        let T::Apply { fun, args: quotes } = *self.tpd.tree(applied) else { return };
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return };
        if !matches!(self.tpd.tree(qualifier), T::Apply { .. }) {
            return;
        }
        let Type::Applied(_, args) = self.u.types().get(self.tpd.tpe(qualifier)).clone() else { return };
        let Some(class) = self.context_function_class(1) else { return };
        let tpe = self.u.intern_type(Type::Applied(class, args));
        let apply = self.tpd.sym(fun).expect("the apply of a context function");
        let apply_tpe = self.ref_type(Some(tpe), apply);
        self.tpd.get_mut(qualifier).tpe = Some(tpe);
        self.tpd.get_mut(fun).tpe = Some(apply_tpe);
        self.tpd.get_mut(applied).span = span;
        if let &[quotes] = self.tpd.items(quotes) {
            self.tpd.get_mut(quotes).span = Span::synthetic(span.start(), span.start());
        }
    }

    /// `scala.quoted.runtime.Expr.<name>(arg)` at `span`, typed.
    fn runtime_call(&mut self, span: Span, name: &str, arg: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let module = TName { name: self.name("scala.quoted.runtime.Expr"), is_type: false };
        let qualifier = self.ast.node(Tree::TypedSplice { sym: module });
        self.ast.with_span(qualifier, span.to_synthetic());
        let name = TName { name: self.names.intern(name), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(fun, span.to_synthetic());
        let args = self.ast.list(&[arg]);
        let call = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(call, span);
        self.typed(call, pt, ctx)
    }
}
