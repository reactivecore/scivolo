// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Extractors that call transparent inline methods, wrapped and expanded while typing (dotty's
//! `inlinedUnapplyFnAndApp`).

use super::super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use rustc_hash::FxHashMap;

impl Typer<'_> {
    /// Extractor function `fun` in `ctx` wrapped as `Inlining::inlined_unapply_fun` wraps it if
    /// it applies a transparent inline method.
    pub fn transparent_unapply(&mut self, fun: TNodeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let Some(meth) = self.tpd.sym(self.tpd.meth_part(fun)).filter(|&m| self.is_transparent_inline(m))
        else {
            return Ok(fun);
        };
        let tpe = self.tpd.tpe(fun);
        if ctx.staging != 0 || self.enclosing_inline(ctx.owner).is_some() || !self.is_fully_defined(tpe) {
            return Err("transparent inline extractor");
        }
        self.define_call_types(fun)?;
        let mut bodies = FxHashMap::default();
        self.collect_transparent_bodies(meth, false, &mut bodies)?;
        let with_inits = self.classes_with_inits();
        let ctx = ctx.with_mode(ctx.mode.without(Mode::PATTERN));
        let wrapped = self.with_inlining(true, |i| i.transparent_unapply_fun((bodies, with_inits), fun, ctx));
        wrapped.map_err(|(_, what)| what)
    }
}
