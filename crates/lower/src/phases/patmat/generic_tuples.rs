// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PatternMatcher.scala
//! The components of an extractor result of a generic tuple type `A *: B *: EmptyTuple`, as
//! `Tuples.apply(result, i)` (dotty's `tupleApp`).

use super::Translator;
use super::plan::PlanId;
use crate::{Result, unsupported};
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};
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
        let Some(elems) = self.generic_tuple_elems(tpe) else { return Ok(None) };
        let components = self.tuple_components(result, &elems)?;
        self.match_args_plan(&components, args, on_success).map(Some)
    }

    /// The element types of `tpe` if it is a chain of `*:` ending in `EmptyTuple`.
    fn generic_tuple_elems(&mut self, tpe: TypeId) -> Option<Vec<TypeId>> {
        let mut out = Vec::new();
        let widened = self.l.u.widen(tpe);
        let mut t = self.l.u.dealias(widened);
        while let Type::Applied(tycon, args) = self.l.u.types().get(t).clone() {
            let class = self.l.u.class_symbol(tycon)?;
            if self.l.u.full_name(class) != "scala.*:" || args.len() != 2 {
                return None;
            }
            out.push(args[0]);
            t = self.l.u.dealias(args[1]);
        }
        let class = match *self.l.u.types().get(t) {
            Type::Sym(_, s) if self.l.u.is_module(s) => self.l.u.module_class(s),
            _ => {
                let widened = self.l.u.widen(t);
                self.l.u.class_symbol(widened)
            }
        };
        let empty =
            class.is_some_and(|c| self.l.u.full_name(c).trim_end_matches('$').ends_with(".EmptyTuple"));
        empty.then_some(out)
    }

    /// `Tuples.apply(result, i)` cast to `elem` for each of `elems`.
    fn tuple_components(&mut self, result: SymbolId, elems: &[TypeId]) -> Result<Vec<TNodeId>> {
        let found = self.l.u.package("scala.runtime").and_then(|runtime| {
            let tuples =
                self.l.u.decl(runtime, "Tuples").ok()?.into_iter().find(|&s| self.l.u.is_module(s))?;
            let class = self.l.u.module_class(tuples)?;
            Some((tuples, *self.l.u.decl(class, "apply").ok()?.first()?))
        });
        let Some((tuples, apply)) = found else { return unsupported(Span::NONE, "`Tuples.apply` not found") };
        let any = self.l.u.class_ref("scala.Any");
        let mut out = Vec::new();
        for (i, &elem) in elems.iter().enumerate() {
            let value = self.l.ident(result, Span::NONE);
            let module_ref = self.l.ident(tuples, Span::NONE);
            let fun = self.l.select(module_ref, apply, Span::NONE);
            let index = self.l.int_literal(i as i32);
            let app = self.l.apply(fun, &[value, index], any, Span::NONE);
            let app = self.l.positioned(app);
            out.push(self.l.ensure_conforms(app, elem));
        }
        Ok(out)
    }
}
