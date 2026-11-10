// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Extractors that call transparent inline methods, wrapped and expanded while typing (dotty's
//! `inlinedUnapplyFnAndApp`).

use super::super::Typer;
use super::UnapplyPattern;
use crate::ctx::Mode;
use crate::tpd::TNodeId;
use rustc_hash::FxHashMap;

impl UnapplyPattern {
    /// Wraps extractor function `fun` as `Inlining::inlined_unapply_fun` wraps it if it applies a
    /// transparent inline method, as `case Macro(x) =>` for `transparent inline def unapply`.
    /// Refuses one inside an inline method or a quote, or whose type is not fully defined.
    pub fn transparent_unapply(&self, t: &mut Typer<'_>, fun: TNodeId) -> Result<TNodeId, &'static str> {
        let Some(meth) = t.tpd.sym(t.tpd.meth_part(fun)).filter(|&m| t.is_transparent_inline(m)) else {
            return Ok(fun);
        };
        let tpe = t.tpd.tpe(fun);
        let ctx = self.ctx;
        if ctx.staging != 0 || t.enclosing_inline(ctx.owner).is_some() || !t.is_fully_defined(tpe) {
            return Err("transparent inline extractor");
        }

        t.define_call_types(fun)?;
        let mut bodies = FxHashMap::default();
        t.collect_transparent_bodies(meth, false, &mut bodies)?;
        let with_inits = t.classes_with_inits();
        let ctx = ctx.with_mode(ctx.mode.without(Mode::PATTERN));
        let wrapped = t.with_inlining(true, |i| i.transparent_unapply_fun((bodies, with_inits), fun, ctx));
        wrapped.map_err(|(_, what)| what)
    }
}
