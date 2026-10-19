//! Choosing among overloaded alternatives applied to type arguments (dotty's `PolyProto` case of
//! `resolveOverloaded`, `canInstantiate`, `resolveMapped`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::NodeId;
use tasty::{SymbolId, Type, TypeId};

const MISS: &str = "overloaded reference with type arguments";

/// The type arguments of a type application, typed once when overloading needs their types, and
/// what the application is expected to be (dotty's `PolyProto`).
pub struct PolyProto {
    pub args: Vec<NodeId>,
    pub ctx: Ctx,
    pub pt: Pt,
    pub typed: Option<Vec<TNodeId>>,
}

impl Typer<'_> {
    /// The alternative of `syms`, members of `prefix`, that the type arguments of poly proto
    /// `poly` select: those taking as many type parameters, within their bounds, resolved with
    /// the arguments substituted against the expected type of the type application.
    pub fn choose_poly(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        poly: usize,
    ) -> Result<SymbolId, &'static str> {
        let n = self.poly_protos[poly].args.len();
        let mut fitting = Vec::new();
        for &s in syms {
            let info = self.alt_info(s, prefix);
            if self.instantiable(info, n) {
                fitting.push(s);
            }
        }
        if fitting.len() < 2 {
            return fitting.first().copied().ok_or(MISS);
        }
        let targs = self.poly_arg_types(poly);
        let within: Vec<SymbolId> = fitting
            .iter()
            .copied()
            .filter(|&s| {
                let info = self.alt_info(s, prefix);
                self.within_bounds(info, s, &targs)
            })
            .collect();
        if within.len() < 2 {
            return within.first().copied().ok_or(MISS);
        }
        let saved = self.mapped_targs.replace((fitting.clone(), targs));
        let chosen = match self.poly_protos[poly].pt {
            pt @ (Pt::Fun(_) | Pt::Any | Pt::Type(_)) => self.choose(&fitting, prefix, pt),
            Pt::PolyFun(outer) => self.poly_matched(&fitting, prefix, outer),
            Pt::AnyFunction => Err(MISS),
        };
        self.mapped_targs = saved;
        chosen
    }

    /// The only alternative of `alts` that can take the type arguments of poly proto `outer`
    /// itself or through an `apply` member of its result (dotty's `PolyProto.isMatchedBy`).
    fn poly_matched(
        &mut self,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
        outer: usize,
    ) -> Result<SymbolId, &'static str> {
        let n = self.poly_protos[outer].args.len();
        let mut found = Vec::new();
        for &s in alts {
            let info = self.alt_info(s, prefix);
            let info = self.mapped_info(info, s);
            let matched = self.instantiable(info, n)
                || !matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. })
                    && self.type_members(info, "apply").into_iter().any(|a| {
                        self.complete(a);
                        let apply = self.u.info(a);
                        self.instantiable(apply, n)
                    });
            if matched {
                found.push(s);
            }
        }
        match found[..] {
            [s] => Ok(s),
            _ => Err(MISS),
        }
    }

    /// The type of alternative `s` seen from `prefix`, of a method its signature.
    fn alt_info(&mut self, s: SymbolId, prefix: Option<TypeId>) -> TypeId {
        self.complete(s);
        let tpe = self.ref_type(prefix, s);
        self.method_info(tpe).unwrap_or(tpe)
    }

    /// Whether `info` is polymorphic of `n` type parameters (dotty's `canInstantiate`).
    fn instantiable(&self, info: TypeId, n: usize) -> bool {
        matches!(self.u.types().get(info), Type::Poly { params, .. } if params.len() == n)
    }

    /// The types of the arguments of poly proto `poly`, typed once.
    fn poly_arg_types(&mut self, poly: usize) -> Vec<TypeId> {
        let typed = match self.poly_protos[poly].typed.clone() {
            Some(typed) => typed,
            None => {
                let (args, ctx) = (self.poly_protos[poly].args.clone(), self.poly_protos[poly].ctx);
                let typed: Vec<TNodeId> = args.iter().map(|&a| self.typed_type(a, ctx)).collect();
                self.poly_protos[poly].typed = Some(typed.clone());
                typed
            }
        };
        typed.iter().map(|&t| self.tpd.tpe(t)).collect()
    }

    /// Whether `targs` are within the bounds of the type parameters of polymorphic type `info`
    /// of `alt`.
    fn within_bounds(&mut self, info: TypeId, alt: SymbolId, targs: &[TypeId]) -> bool {
        let Type::Poly { binder, params, .. } = self.u.types().get(info).clone() else { return false };
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let before = self.constraint.attempt();
        let mut within = true;
        for (&(_, bounds), &arg) in params.iter().zip(targs) {
            let bounds = self.instantiate_poly(bounds, binder, &names, targs, Some(alt));
            let (lo, hi, arity) = self.bounds_of(bounds);
            within &= self.type_arity(arg) == arity
                && self.conforms(arg, hi)
                && lo.is_none_or(|lo| self.conforms(lo, arg));
        }
        self.constraint.abandon(before);
        within
    }

    /// The type arguments `alt` is resolved with, if it is an alternative of the resolution with
    /// instantiated type parameters in progress and takes `n` type parameters.
    pub fn mapped_targs_of(&self, alt: SymbolId, n: usize) -> Option<Vec<TypeId>> {
        let (alts, targs) = self.mapped_targs.as_ref()?;
        (alts.contains(&alt) && targs.len() == n).then(|| targs.clone())
    }

    /// `arg` typed against `pt` with no alternatives resolved with instantiated type parameters.
    pub fn typed_unmapped(&mut self, arg: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let mapped = self.mapped_targs.take();
        let typed = self.typed(arg, pt, ctx);
        self.mapped_targs = mapped;
        typed
    }

    /// Polymorphic type `info` of `alt` instantiated if `alt` is resolved with type arguments,
    /// else `info`.
    pub fn mapped_info(&mut self, info: TypeId, alt: SymbolId) -> TypeId {
        match self.u.types().get(info) {
            Type::Poly { params, .. } if self.mapped_targs_of(alt, params.len()).is_some() => {
                self.poly_instance(info, alt, false)
            }
            _ => info,
        }
    }
}
