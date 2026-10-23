// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/QuotesAndSplices.scala
//! Top-level splices in the unit's inline methods and the term quotes in them, as dotty types
//! them: `${ e }` as `scala.quoted.runtime.Expr.splice(e)`, `'{ e }` as
//! `scala.quoted.runtime.Expr.quote(e)` applied to the splice's `Quotes`.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, TName, Tree};
use tasty::Kind;
use tasty::tags::INLINE;

impl Typer<'_> {
    /// `${ expr }`, a splice of inline method `ctx.owner` outside quotes.
    pub fn typed_splice(&mut self, id: NodeId, expr: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let owner = self.u.symbol(ctx.owner);
        if ctx.mode.is(Mode::SPLICE) || owner.kind != Kind::Def || !owner.flags.has(INLINE) {
            return self.unsupported(id, "Splice");
        }
        self.runtime_call(id, "splice", expr, pt, ctx.with_mode(ctx.mode.with(Mode::SPLICE)))
    }

    /// `'{ body }`, a term quote in a splice.
    pub fn typed_quote(&mut self, id: NodeId, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if !ctx.mode.is(Mode::SPLICE) || self.ast.is_type(body) {
            return self.unsupported(id, "Quote");
        }
        self.runtime_call(id, "quote", body, pt, ctx)
    }

    /// `scala.quoted.runtime.Expr.<name>(arg)` at the span of `id`, typed.
    fn runtime_call(&mut self, id: NodeId, name: &str, arg: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id).to_synthetic();
        let module = TName { name: self.name("scala.quoted.runtime.Expr"), is_type: false };
        let qualifier = self.ast.node(Tree::TypedSplice { sym: module });
        self.ast.with_span(qualifier, span);
        let name = TName { name: self.names.intern(name), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(fun, span);
        let args = self.ast.list(&[arg]);
        let call = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(call, span);
        self.typed(call, pt, ctx)
    }
}
