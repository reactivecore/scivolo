// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PatternMatcher.scala
//! The components of an extractor result of a generic tuple type `A *: B *: EmptyTuple`, as
//! `Tuples.apply(result, i)` (dotty's `tupleApp`).

use super::super::tuple_optimizations::elements::tuple_element_types;
use super::Translator;
use super::plan::PlanId;
use crate::{Result, unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::TNodeId;

impl Translator<'_, '_> {
    /// The plan matching the components of extractor result `result` against `args` if it is a
    /// generic tuple.
    pub fn generic_tuple_plan(
        &mut self,
        result: SymbolId,
        args: &[TNodeId],
        on_success: PlanId,
    ) -> Result<Option<PlanId>> {
        let tpe = self.l.u.info(result);
        let Some(elems) = tuple_element_types(self.l, tpe).filter(|e| !e.is_empty()) else {
            return Ok(None);
        };
        let components = self.tuple_components(result, elems.len())?;
        self.match_args_plan(&components, args, on_success).map(Some)
    }

    /// `Tuples.apply(result, i)` for each of the `n` elements.
    fn tuple_components(&mut self, result: SymbolId, n: usize) -> Result<Vec<TNodeId>> {
        let found = self.l.u.package("scala.runtime").and_then(|runtime| {
            let tuples =
                self.l.u.decl(runtime, "Tuples").ok()?.into_iter().find(|&s| self.l.u.is_module(s))?;
            let class = self.l.u.module_class(tuples)?;
            Some((tuples, *self.l.u.decl(class, "apply").ok()?.first()?))
        });
        let Some((tuples, apply)) = found else { return unsupported(Span::NONE, "`Tuples.apply` not found") };
        let any = self.l.u.class_ref("scala.Any");
        let mut out = Vec::new();
        for i in 0..n {
            let value = self.l.ident(result, Span::NONE);
            let module_ref = self.l.ident(tuples, Span::NONE);
            let fun = self.l.select(module_ref, apply, Span::NONE);
            let index = self.l.int_literal(i as i32);
            let app = self.l.apply(fun, &[value, index], any, Span::NONE);
            out.push(self.l.positioned(app));
        }
        Ok(out)
    }
}
