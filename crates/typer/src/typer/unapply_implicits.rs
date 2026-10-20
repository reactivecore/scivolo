// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The implicit clause of an extractor, `unapply(x)(using …)` (dotty's `typedUnApply` typing
//! `unapplyFn(dummyArg)` and its `unapplyImplicits`).

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The result of extractor `unapp` applied to a value of type `own`, its implicit clause, of
    /// result type `result`, applied to the arguments the search finds, and those arguments.
    pub fn unapply_implicits(
        &mut self,
        unapp: TNodeId,
        result: TypeId,
        own: TypeId,
        ctx: Ctx,
    ) -> Result<(TypeId, Vec<TNodeId>), &'static str> {
        if !matches!(self.u.types().get(result), Type::Method { implicit: true, .. }) {
            return Ok((result, Vec::new()));
        }
        let span = self.tpd.span(unapp);
        let dummy = self.build().literal(Constant::Null, span.end_pos());
        self.tpd.get_mut(dummy).tpe = Some(own);
        let app = self.build().apply(unapp, &[dummy], result, span);
        let applied = self.implicit_clause_applied(app, ctx.with_mode(Mode::NONE))?;
        let T::Apply { fun, args } = *self.tpd.tree(applied) else { return Err("extractor with implicits") };
        if fun != app {
            return Err("extractor with lifted implicit arguments");
        }
        let tpe = self.tpd.tpe(applied);
        if matches!(self.u.types().get(tpe), Type::Method { .. } | Type::Poly { .. }) {
            return Err("extractor with more parameters");
        }
        Ok((tpe, self.tpd.items(args).to_vec()))
    }
}
