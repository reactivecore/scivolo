// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! The expected result type of an application: the result constrained by it (dotty's
//! `constrainResult`), the results of alternatives compared with it (`resultConforms`).

use super::dependent_params::{applied_result, clause_param, mentions_clause_params, substituted};
use super::{Pt, Typer};
use crate::ctx::Ctx;
use tasty::{Binder, NameId, SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Approximates result `result` of the clause of binder `binder` and parameters `params` of
    /// method `meth` by replacing each parameter with a fresh type variable below its type and
    /// `Singleton`, only `Singleton` where its type refers to the clause (dotty's
    /// `resultTypeApprox` with `newDepTypeVar`), with those variables; `None` unless `pt` is a
    /// type.
    pub fn result_type_approx(
        &mut self,
        result: TypeId,
        (binder, params): (Binder, &[(NameId, TypeId)]),
        meth: Option<SymbolId>,
        pt: Pt,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        let Pt::Type(_) = pt else { return None };

        let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
        let singleton = crate::defn::type_ref(self.u, self.u.defn().singleton);
        let mut vars = Vec::with_capacity(params.len());
        for &(_, p) in params {
            let var = self.constraint.fresh(self.u);
            let p = self.u.by_name(p).unwrap_or(p);
            let hi = match mentions_clause_params(self.u, p, (binder, &names), meth) {
                true => singleton,
                false => self.u.intern_type(Type::And(p, singleton)),
            };
            if let Some(v) = self.u.var(var) {
                self.constraint.add_upper(self.u, v, hi);
            }
            vars.push(var);
        }

        let param = |u: &tasty::Universe, part: TypeId| clause_param(u, part, (binder, &names), meth);
        Some((substituted(self.u, result, &param, &vars), vars))
    }

    /// Constrains the result type `result` of an application of `meth` to conform to `pt`
    /// (dotty's `constrainResult`), only necessary constraints kept: normalized against a fully
    /// defined `pt`, not at all against `Unit`, else a value result after the implicit clauses,
    /// approximated by wildcards where it refers to their parameters.
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
        self.refinement_result((result, meth), pt)?;
        if defined && self.u.class_symbol(p) == Some(self.defn.unit) {
            return Ok(());
        }
        let follow = !self.is_context_function_type(p);
        let value = match defined {
            true => self.proto_normalized((result, meth), p, follow, 8),
            false => {
                let value = match self.refers_to_implicit_params(result, stripped, meth) {
                    true => self.wildcard_result(result, meth).map(|approx| self.zonk(approx)),
                    false => Some(stripped),
                };
                value
                    .filter(|&r| !matches!(self.u.ty(r), Type::Method { .. } | Type::Poly { .. }))
                    .map(|r| if follow { self.context_result(r) } else { r })
            }
        };
        if let Some(value) = value {
            self.u.is_necessary_subtype_in(value, p, &mut self.constraint);
        }
        Ok(())
    }

    /// Refuses implicit method type `result` of `meth` under a `pt` with type variables whose
    /// result selects a type member only a refinement has, or a higher-kinded member of an
    /// implicit parameter unapplied (no wildcard approximation).
    pub fn refinement_result(
        &mut self,
        (result, meth): (TypeId, Option<SymbolId>),
        pt: Pt,
    ) -> Result<(), &'static str> {
        let Pt::Type(p) = pt else { return Ok(()) };
        let stripped = self.strip_implicit(result);
        if stripped == result || self.is_fully_defined(p) {
            return Ok(());
        }
        if self.wildcard_result(result, meth).is_none() {
            return Err(
                "implicit method result with a higher-kinded member of an implicit parameter unapplied",
            );
        }

        let mut found = false;
        self.u.map_type(stripped, &mut |u, part| {
            found |= u.type_ref(part).is_some() && u.resolve_ref(part).is_none();
            None
        });
        match found {
            true => Err("implicit method result selecting a refinement member under type variables"),
            false => Ok(()),
        }
    }

    /// Whether `stripped` refers to a parameter of the leading implicit clauses of `t`.
    fn refers_to_implicit_params(&mut self, mut t: TypeId, stripped: TypeId, meth: Option<SymbolId>) -> bool {
        while let Type::Method { binder, result, implicit: true, .. } = *self.u.ty(t) {
            if self.mentions_params(stripped, binder, meth) {
                return true;
            }
            t = result;
        }
        false
    }

    /// `t` as a value compared with `pt` (dotty's `normalize`): type parameters made type
    /// variables, implicit clauses stripped, a by-name type's result, a method as a function
    /// type, a context function's result unless `t` itself is one and not `follow` (`followIFT`);
    /// `None` where outside the subset (dependent methods). The parameters of `meth`'s implicit
    /// clauses are approximated by wildcards (`wildcard_result`).
    pub fn proto_normalized(
        &mut self,
        (t, meth): (TypeId, Option<SymbolId>),
        pt: TypeId,
        follow: bool,
        depth: u32,
    ) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        match *self.u.ty(t) {
            Type::Poly { binder, params, result } => {
                let names: Vec<NameId> = self.u.params_of(params).iter().map(|&(n, _)| n).collect();
                let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
                let result = self.instantiate_poly(result, binder, &names, &vars, None);
                self.proto_normalized((result, meth), pt, true, depth)
            }
            Type::Method { implicit: true, .. } => {
                let result = self.wildcard_result(t, meth)?;
                self.proto_normalized((result, meth), pt, true, depth)
            }
            Type::Method { binder, params, result, .. } => {
                let mut dependent = false;
                self.u.map_type(result, &mut |u, p| {
                    dependent |= u.param_ref(p).is_some_and(|(b, _)| b == binder);
                    None
                });
                if dependent {
                    return None;
                }
                let rt = self.proto_normalized((result, meth), pt, true, depth)?;
                let mut args: Vec<TypeId> = self.u.params_of(params).iter().map(|&(_, p)| p).collect();
                let empty = args.is_empty();
                args.push(rt);
                let class = self.function_class(args.len() - 1)?;
                let ft = self.u.intern_applied(class, &args);
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
    /// compatible with `pt` in `ctx`, through an implicit conversion if `views`, its type
    /// parameters made type variables; the constraint unchanged; `None` if undecided.
    pub fn result_conforms(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        pt: TypeId,
        (views, ctx): (bool, Ctx),
    ) -> Option<bool> {
        if self.u.class_symbol(pt) == Some(self.defn.unit) {
            return Some(true);
        }
        let follow = !self.is_context_function_type(pt);
        let before = self.constraint.attempt();
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let mut info = self.method_info(tpe).unwrap_or(tpe);
        if let Some((binder, result, names)) = self.u.poly(info).map(|p| (p.binder, p.result, p.names())) {
            let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
            info = self.instantiate_poly(result, binder, &names, &vars, Some(alt));
        }
        let fits = match self.u.method(info).map(|m| (m.binder, m.params.to_vec(), m.result)) {
            Some((binder, params, result)) => {
                let result = self.params_as_skolems(result, (binder, &params), alt);
                match self.proto_normalized((result, Some(alt)), pt, follow, 8) {
                    Some(value) if self.u.is_necessary_subtype_in(value, pt, &mut self.constraint) => {
                        Some(true)
                    }
                    Some(value) => self.compatible(value, pt, views, ctx),
                    None => None,
                }
            }
            None => Some(true),
        };
        self.constraint.abandon(before);
        fits
    }

    /// Result `result` of method `alt` of binder `binder` and parameters `params`, each reference
    /// to a parameter replaced by a skolem of its type, as dotty's `TermParamRef`s compare.
    fn params_as_skolems(
        &mut self,
        result: TypeId,
        (binder, params): (Binder, &[(NameId, TypeId)]),
        alt: SymbolId,
    ) -> TypeId {
        let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
        let formals: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
        applied_result(self.u, result, (binder, &names), Some(alt), &formals)
    }
}
