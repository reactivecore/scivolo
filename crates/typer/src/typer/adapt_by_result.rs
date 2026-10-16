//! The expected result type of an application: the result constrained by it (dotty's
//! `constrainResult`), overload resolution revisited by it (`adaptByResult`, `resultConforms`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId};

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
    /// else a value result only; `Err` for an implicit method whose result is a type variable
    /// under a `pt` with type variables.
    pub fn constrain_applied_result(
        &mut self,
        meth: Option<SymbolId>,
        result: TypeId,
        pt: Pt,
    ) -> Result<(), &'static str> {
        let Pt::Type(p) = pt else { return Ok(()) };
        let p = self.result_pt(meth, p);
        let defined = self.is_fully_defined(p);
        let stripped = self.strip_implicit(result);
        let stripped = self.zonk(stripped);
        if !defined && stripped != result && matches!(self.u.types().get(stripped), Type::Var(_)) {
            return Err("implicit method result under a type with type variables");
        }
        let follow = !self.is_context_function_type(p);
        let value = match defined && self.u.class_symbol(p) != Some(self.defn.unit) {
            true => self.proto_normalized((result, meth), p, follow, 8),
            false => Some(result)
                .filter(|&r| !matches!(self.u.types().get(r), Type::Method { .. } | Type::Poly { .. }))
                .map(|r| if follow { self.context_result(r) } else { r }),
        };
        if let Some(value) = value {
            self.conforms(value, p);
        }
        Ok(())
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
    /// type, a context function's result unless `t` itself is one and not `follow`; `None` where
    /// outside the subset (dependent methods). The parameters of `meth`'s implicit clauses that
    /// the result refers to are approximated by their types (dotty's `resultTypeApprox`).
    fn proto_normalized(
        &mut self,
        (t, meth): (TypeId, Option<SymbolId>),
        pt: TypeId,
        follow: bool,
        depth: u32,
    ) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        match self.u.types().get(t).clone() {
            Type::Poly { binder, params, result } => {
                let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
                let result = self.instantiate_poly(result, binder, &names, &vars, None);
                self.proto_normalized((result, meth), pt, true, depth)
            }
            Type::Method { binder, params, result, implicit: true, .. } => {
                let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                let infos: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
                let result = self.u.subst(result, binder, &names, &infos);
                let result = self.u.map_type(result, &mut |u, t| match *u.types().get(t) {
                    Type::Sym(None, s) if u.symbol(s).kind == tasty::Kind::Param && meth.is_some() => {
                        let own = u.symbol(s).owner == meth;
                        names.iter().position(|n| own && *n == u.symbol(s).name).map(|i| infos[i])
                    }
                    _ => None,
                });
                self.proto_normalized((result, meth), pt, true, depth)
            }
            Type::Method { binder, params, result, .. } => {
                let mut dependent = false;
                self.u.map_type(result, &mut |u, p| {
                    dependent |= matches!(u.types().get(p), Type::Param(b, _) if *b == binder);
                    None
                });
                if dependent {
                    return None;
                }
                let rt = self.proto_normalized((result, meth), pt, true, depth)?;
                let mut args: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
                let empty = args.is_empty();
                args.push(rt);
                let class = self.function_class(params.len())?;
                let ft = self.u.intern_type(Type::Applied(class, args));
                Some(if !empty || self.u.is_subtype(ft, pt) { ft } else { rt })
            }
            Type::ByName(r) => self.proto_normalized((r, meth), pt, true, depth),
            _ => match self.context_function_parts(t) {
                Some((_, result)) if follow => self.proto_normalized((result, meth), pt, true, depth),
                _ => Some(t),
            },
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
        let follow = !self.is_context_function_type(pt);
        let before = self.constraint.attempt();
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let mut info = self.method_info(tpe).unwrap_or(tpe);
        if let Type::Poly { binder, params, result } = self.u.types().get(info).clone() {
            let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
            let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
            info = self.instantiate_poly(result, binder, &names, &vars, Some(alt));
        }
        let fits = match *self.u.types().get(info) {
            Type::Method { result, .. } => match self.proto_normalized((result, Some(alt)), pt, follow, 8) {
                Some(value) => self.compatible(value, pt, true, ctx),
                None => None,
            },
            _ => Some(true),
        };
        self.constraint.abandon(before);
        fits
    }
}
