//! The expected result type of an application: the result constrained by it (dotty's
//! `constrainResult`), overload resolution revisited by it (`adaptByResult`, `resultConforms`).

use super::{Pt, Typer};
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

    /// Constrains the result type `result` of an application of `meth` to conform to `pt`
    /// (dotty's `constrainResult`): normalized against a fully defined `pt` other than `Unit`,
    /// else a value result only.
    pub fn constrain_applied_result(&mut self, meth: Option<SymbolId>, result: TypeId, pt: Pt) {
        let Pt::Type(p) = pt else { return };
        let p = self.result_pt(meth, p);
        let value = match self.is_fully_defined(p) && self.u.class_symbol(p) != Some(self.defn.unit) {
            true => self.proto_normalized(result, p, 8),
            false => Some(result)
                .filter(|&r| !matches!(self.u.types().get(r), Type::Method { .. } | Type::Poly { .. })),
        };
        if let Some(value) = value {
            self.conforms(value, p);
        }
    }

    /// `t` without its leading implicit parameter clauses (dotty's `stripImplicit`).
    pub fn strip_implicit(&self, mut t: TypeId) -> TypeId {
        while let Type::Method { result, implicit: true, .. } = *self.u.types().get(t) {
            t = result;
        }
        t
    }

    /// `t` as a value compared with `pt` (dotty's `normalize`): type parameters made type
    /// variables, implicit clauses stripped, a by-name type's result, a method as a function
    /// type; `None` where outside the subset (dependent methods, context function results).
    fn proto_normalized(&mut self, t: TypeId, pt: TypeId, depth: u32) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        match self.u.types().get(t).clone() {
            Type::Poly { binder, params, result } => {
                let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
                let result = self.instantiate_poly(result, binder, &names, &vars, None);
                self.proto_normalized(result, pt, depth)
            }
            Type::Method { result, implicit: true, .. } => self.proto_normalized(result, pt, depth),
            Type::Method { binder, params, result, .. } => {
                let mut dependent = false;
                self.u.map_type(result, &mut |u, p| {
                    dependent |= matches!(u.types().get(p), Type::Param(b, _) if *b == binder);
                    None
                });
                if dependent {
                    return None;
                }
                let rt = self.proto_normalized(result, pt, depth)?;
                let mut args: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
                let empty = args.is_empty();
                args.push(rt);
                let class = self.function_class(params.len())?;
                let ft = self.u.intern_type(Type::Applied(class, args));
                Some(if !empty || self.u.is_subtype(ft, pt) { ft } else { rt })
            }
            Type::ByName(r) => self.proto_normalized(r, pt, depth),
            _ => {
                let context = self
                    .u
                    .class_symbol(t)
                    .is_some_and(|c| self.u.full_name(c).starts_with("scala.ContextFunction"));
                (!context).then_some(t)
            }
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
            Type::Method { result, .. } => match self.proto_normalized(result, pt, 8) {
                Some(value) => self.compatible(value, pt, true, ctx),
                None => None,
            },
            _ => Some(true),
        };
        self.constraint.rollback(mark);
        fits
    }
}
