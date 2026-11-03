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
    /// Extractor `unapp` with the implicit clauses before its scrutinee's applied to the
    /// arguments the search finds (dotty's `adapt` of `unapplyFn` to `FunProto(dummyArg)`).
    pub fn leading_implicits_applied(
        &mut self,
        mut unapp: TNodeId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        loop {
            let tpe = self.tpd.tpe(unapp);
            let info = self.method_info(tpe).unwrap_or(tpe);
            if !matches!(self.u.types().get(info), Type::Method { implicit: true, .. }) {
                return Ok(unapp);
            }
            unapp = self.implicit_clause_applied(unapp, ctx.with_mode(Mode::NONE))?;
        }
    }

    /// The result of extractor `unapp` applied to a value of type `own`, its implicit clauses, the
    /// first of result type `result`, applied to the arguments the search finds, and those
    /// arguments in one list.
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
        let mut app = self.build().apply(unapp, &[dummy], result, span);
        let mut implicits = Vec::new();
        loop {
            let applied = self.implicit_clause_applied(app, ctx.with_mode(Mode::NONE))?;
            let T::Apply { fun, args } = *self.tpd.tree(applied) else {
                return Err("extractor with implicits");
            };
            if fun != app {
                return Err("extractor with lifted implicit arguments");
            }
            implicits.extend_from_slice(self.tpd.items(args));
            let tpe = self.tpd.tpe(applied);
            match self.u.types().get(tpe) {
                Type::Method { implicit: true, .. } => app = applied,
                Type::Method { .. } | Type::Poly { .. } => return Err("extractor with more parameters"),
                _ => return Ok((tpe, implicits)),
            }
        }
    }
}
