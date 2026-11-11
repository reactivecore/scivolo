// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Choosing among overloaded alternatives applied to type arguments (dotty's `PolyProto` case of
//! `resolveOverloaded1`, `canInstantiate`, `resolveMapped`).

use super::poly_instances::mapped_info;
use super::{Resolution, Unchosen};
use crate::tpd::TNodeId;
use crate::typer::{Pt, Typer};
use tasty::{NameId, SymbolId, Type, TypeId, Universe};

/// Why type arguments select no alternative, or several.
const MISS: &str = "overloaded reference with type arguments";

impl Resolution {
    /// Chooses the alternative of `syms` that the type arguments of poly proto `poly` select,
    /// as `f[Int]` among `def f[A](a: A)` and `def f[A, B](a: A, b: B)`: those taking as many
    /// type parameters, within their bounds, resolved with the arguments substituted against the
    /// expected type of the type application.
    pub fn choose_poly(self, t: &mut Typer, syms: &[SymbolId], poly: usize) -> Result<SymbolId, Unchosen> {
        let n = t.poly_protos[poly].args.len();
        let mut fitting = Vec::new();
        for &s in syms {
            let info = self.alt_info(t, s);
            if instantiable(t.u, info, n) {
                fitting.push(s);
            }
        }
        if fitting.len() < 2 {
            return fitting.first().copied().ok_or_else(|| MISS.into());
        }

        let targs = poly_arg_types(t, poly);
        let within: Vec<SymbolId> = fitting
            .iter()
            .copied()
            .filter(|&s| {
                let info = self.alt_info(t, s);
                within_bounds(t, info, s, &targs)
            })
            .collect();
        if within.len() < 2 {
            return within.first().copied().ok_or_else(|| MISS.into());
        }

        let saved = t.mapped_targs.replace((fitting.clone(), targs));
        let chosen = match t.poly_protos[poly].pt {
            pt @ (Pt::Fun(_) | Pt::Any | Pt::Type(_)) => Resolution { pt, ..self }.choose(t, &fitting),
            Pt::PolyFun(outer) => self.poly_matched(t, &fitting, outer).map_err(Unchosen::from),
            Pt::AnyFunction => Err(MISS.into()),
        };
        t.mapped_targs = saved;
        chosen
    }

    /// Finds the only alternative of `alts` that can take the type arguments of poly proto
    /// `outer` itself or through an `apply` member of its result (dotty's
    /// `PolyProto.isMatchedBy`), as `f[Int][String]`.
    fn poly_matched(self, t: &mut Typer, alts: &[SymbolId], outer: usize) -> Result<SymbolId, &'static str> {
        let n = t.poly_protos[outer].args.len();
        let mut found = Vec::new();
        for &s in alts {
            let info = self.alt_info(t, s);
            let info = mapped_info(t, info, s);
            let matched = instantiable(t.u, info, n)
                || !matches!(t.u.ty(info), Type::Method { .. } | Type::Poly { .. })
                    && t.type_members(info, "apply").into_iter().any(|a| {
                        t.complete(a);
                        let apply = t.u.info(a);
                        instantiable(t.u, apply, n)
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

    /// Gives the type of alternative `s` seen from the prefix, of a method its signature.
    fn alt_info(self, t: &mut Typer, s: SymbolId) -> TypeId {
        t.complete(s);
        let tpe = t.ref_type(self.prefix, s);
        t.method_info(tpe).unwrap_or(tpe)
    }
}

/// Checks whether `info` is polymorphic of `n` type parameters (dotty's `canInstantiate`).
fn instantiable(u: &Universe, info: TypeId, n: usize) -> bool {
    u.poly(info).is_some_and(|p| p.params.len() == n)
}

/// Gives the types of the arguments of poly proto `poly`, typed once.
fn poly_arg_types(t: &mut Typer, poly: usize) -> Vec<TypeId> {
    let typed = match t.poly_protos[poly].typed.clone() {
        Some(typed) => typed,
        None => {
            let (args, ctx) = (t.poly_protos[poly].args.clone(), t.poly_protos[poly].ctx);
            let typed: Vec<TNodeId> = args.iter().map(|&a| t.typed_type(a, ctx)).collect();
            t.poly_protos[poly].typed = Some(typed.clone());
            typed
        }
    };
    typed.iter().map(|&a| t.tpd.tpe(a)).collect()
}

/// Checks whether `targs` are within the bounds of the type parameters of polymorphic type
/// `info` of `alt`; the constraint unchanged.
fn within_bounds(t: &mut Typer, info: TypeId, alt: SymbolId, targs: &[TypeId]) -> bool {
    let Some((binder, params)) = t.u.poly(info).map(|p| (p.binder, p.params.to_vec())) else { return false };
    let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
    let before = t.constraint.attempt();
    let mut within = true;
    for (&(_, bounds), &arg) in params.iter().zip(targs) {
        let bounds = t.instantiate_poly(bounds, binder, &names, targs, Some(alt));
        let (lo, hi, arity) = t.bounds_of(bounds);
        within &=
            t.type_arity(arg) == arity && t.conforms(arg, hi) && lo.is_none_or(|lo| t.conforms(lo, arg));
    }
    t.constraint.abandon(before);
    within
}
