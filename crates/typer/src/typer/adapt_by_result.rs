// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! The expected result type of an application: the result constrained by it (dotty's
//! `constrainResult`), the results of alternatives compared with it (`resultConforms`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use tasty::{NameId, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Constrains the result type `result` of an application of `meth` to conform to `pt`
    /// (dotty's `constrainResult`), only necessary constraints kept: normalized against a fully
    /// defined `pt`, not at all against `Unit`, else a value result after the implicit clauses
    /// that refers to none of their parameters.
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
        self.refinement_result(result, pt)?;
        if defined && self.u.class_symbol(p) == Some(self.defn.unit) {
            return Ok(());
        }
        let follow = !self.is_context_function_type(p);
        let value = match defined {
            true => self.proto_normalized((result, meth), p, follow, 8),
            false => Some(stripped)
                .filter(|&r| !matches!(self.u.ty(r), Type::Method { .. } | Type::Poly { .. }))
                .filter(|&r| !self.refers_to_implicit_params(result, r, meth))
                .map(|r| if follow { self.context_result(r) } else { r }),
        };
        if let Some(value) = value {
            self.u.is_necessary_subtype_in(value, p, &mut self.constraint);
        }
        Ok(())
    }

    /// Refuses implicit method type `result` whose result selects a type member only a
    /// refinement has under a `pt` with type variables (dotty constrains by its approximation).
    pub fn refinement_result(&mut self, result: TypeId, pt: Pt) -> Result<(), &'static str> {
        let Pt::Type(p) = pt else { return Ok(()) };
        let stripped = self.strip_implicit(result);
        if stripped == result || self.is_fully_defined(p) {
            return Ok(());
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
    /// `None` where outside the subset (dependent methods). The parameters of `meth`'s implicit clauses that
    /// the result refers to are approximated by their types (dotty's `resultTypeApprox`).
    pub fn proto_normalized(
        &mut self,
        (t, meth): (TypeId, Option<SymbolId>),
        pt: TypeId,
        follow: bool,
        depth: u32,
    ) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        match self.u.ty(t).clone() {
            Type::Poly { binder, params, result } => {
                let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
                let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
                let result = self.instantiate_poly(result, binder, &names, &vars, None);
                self.proto_normalized((result, meth), pt, true, depth)
            }
            Type::Method { binder, params, result, implicit: true, .. } => {
                let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
                let infos: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
                let result = self.u.subst(result, binder, &names, &infos);
                let result = self.u.map_type(result, &mut |u, t| match *u.ty(t) {
                    Type::Sym(None, s) if u.symbol(s).kind == tasty::Kind::Param && meth.is_some() => {
                        let own = u.symbol(s).owner == meth;
                        names.iter().position(|n| own && u.name(*n) == u.symbol(s).name).map(|i| infos[i])
                    }
                    _ => None,
                });
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
        let fits = match self.u.method(info).map(|m| m.result) {
            Some(result) => match self.proto_normalized((result, Some(alt)), pt, follow, 8) {
                Some(value) if self.u.is_necessary_subtype_in(value, pt, &mut self.constraint) => Some(true),
                Some(value) => self.compatible(value, pt, views, ctx),
                None => None,
            },
            None => Some(true),
        };
        self.constraint.abandon(before);
        fits
    }
}
