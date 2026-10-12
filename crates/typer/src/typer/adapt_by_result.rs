//! Overload resolution revisited by the expected result type (dotty's `adaptByResult` and
//! `resultConforms`).

use super::Typer;
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// `chosen`, or if its result does not conform to the expected result type of `proto`, the
    /// one alternative of `alts` whose result does, or the one resolution picks among them.
    pub fn adapted_by_result(
        &mut self,
        chosen: SymbolId,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
        proto: usize,
    ) -> SymbolId {
        let (Some(pt), ctx) = (self.protos[proto].result, self.protos[proto].ctx) else { return chosen };
        if self.result_conforms(chosen, prefix, pt, ctx) != Some(false) {
            return chosen;
        }
        let conforming: Vec<SymbolId> = alts
            .iter()
            .copied()
            .filter(|&a| a != chosen && self.result_conforms(a, prefix, pt, ctx) == Some(true))
            .collect();
        match conforming[..] {
            [] => chosen,
            [alt] => alt,
            _ => self.resolve_overloaded(&conforming, prefix, proto).unwrap_or(chosen),
        }
    }

    /// Whether the result of `alt` after its first parameter clause and its implicit clauses is
    /// compatible with `pt` in `ctx`, its type parameters made type variables; the constraint
    /// unchanged; `None` if undecided.
    fn result_conforms(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        pt: TypeId,
        ctx: Ctx,
    ) -> Option<bool> {
        if self.u.class_symbol(pt) == Some(self.defn.unit) {
            return Some(true);
        }
        let mark = self.constraint.mark();
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let mut info = self.method_info(tpe).unwrap_or(tpe);
        if let Type::Poly { binder, params, result } = self.u.types().get(info).clone() {
            let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
            let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
            info = self.instantiate_poly(result, binder, &names, &vars, Some(alt));
        }
        let fits = match *self.u.types().get(info) {
            Type::Method { mut result, .. } => {
                while let Type::Method { result: r, implicit: true, .. } = *self.u.types().get(result) {
                    result = r;
                }
                self.compatible(result, pt, true, ctx)
            }
            _ => Some(true),
        };
        self.constraint.rollback(mark);
        fits
    }
}
