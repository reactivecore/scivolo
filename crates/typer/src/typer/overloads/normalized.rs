// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Alternatives of a reference applied to no arguments, compared with the expected type as
//! functions or values (dotty's `normalize`, `normalizedCompatible`).

use super::UNDECIDED;
use super::compatible::is_compatible;
use super::mapped::strip_implicit_clauses;
use super::poly_instances::poly_instance;
use crate::ctx::Ctx;
use crate::typer::{Pt, Typer};
use tasty::{Binder, Kind, SymbolId, Type, TypeId, Universe};

/// Why a nullary method is not compared with an expected type that has type variables in it.
const NULLARY_AGAINST_VARS: &str = "nullary overloaded method against an undetermined type";

/// Checks whether alternative `alt` of type `tp`, normalized, fits `pt`: it conforms to it, or
/// converts to it if `views`, or `pt` is `Unit` (dotty's `normalizedCompatible` without keeping
/// the constraint); `Pt::Any` and `Pt::AnyFunction` take every alternative.
pub fn normalized_compatible(
    t: &mut Typer,
    (alt, tp): (SymbolId, TypeId),
    pt: Pt,
    views: bool,
    ctx: Ctx,
) -> Result<bool, &'static str> {
    let before = t.constraint.attempt();
    let fits = normalized(t, tp, alt, pt).and_then(|norm| match pt {
        Pt::Type(p) => {
            let unit = t.unit_type();
            let p_norm = t.zonk(p);
            let to_unit = t.u.dealias(p_norm) == unit && t.u.method(norm).is_none();
            Ok(to_unit || is_compatible(t, norm, p, views, ctx).ok_or(UNDECIDED)?)
        }
        _ => Ok(true),
    });
    t.constraint.abandon(before);
    fits
}

/// Normalizes type `tp` of alternative `alt` for expected type `pt`: type parameters become type
/// variables, leading implicit clauses are dropped, a method with parameters becomes the function
/// type it expands to, a nullary one that function type if it conforms to `pt` else its result,
/// a by-name or context function type its result (dotty's `normalize`).
pub fn normalized(t: &mut Typer, tp: TypeId, alt: SymbolId, pt: Pt) -> Result<TypeId, &'static str> {
    match *t.u.ty(tp) {
        Type::Poly { .. } => {
            let instance = poly_instance(t, tp, alt, false);
            normalized(t, instance, alt, pt)
        }
        Type::Method { implicit: true, .. } => {
            let (approx, _) = strip_implicit_clauses(t.u, tp, alt);
            normalized(t, approx, alt, pt)
        }
        Type::Method { binder, params, result, .. } => {
            if mentions_params(t.u, result, binder, alt) {
                return Ok(tp);
            }
            let rt = normalized(t, result, alt, pt)?;
            let mut parts: Vec<TypeId> = t.u.params_of(params).iter().map(|&(_, p)| p).collect();
            let nullary = parts.is_empty();
            parts.push(rt);
            let ft = t.function_of(parts)?;
            Ok(if !nullary || nullary_fits(t, ft, pt)? { ft } else { rt })
        }
        Type::ByName(r) => normalized(t, r, alt, pt),
        _ => match t.context_function_parts(tp) {
            Some((_, r)) => normalized(t, r, alt, pt),
            None => Ok(tp),
        },
    }
}

/// Checks whether alternative type `tp` of `alt` stays a method when its type parameters and
/// leading implicit clauses are dropped (dotty's `normalize` against an `IgnoredProto`).
pub fn is_method_ignoring_pt(u: &Universe, mut tp: TypeId) -> bool {
    loop {
        match *u.ty(tp) {
            Type::Poly { result, .. }
            | Type::Method { result, implicit: true, .. }
            | Type::ByName(result) => {
                tp = result;
            }
            Type::Method { .. } => return true,
            _ => return false,
        }
    }
}

/// Checks whether function type `ft` of a nullary method conforms to `pt` without constraining
/// it (dotty's `frozen_<:<`); refuses a `pt` it conforms to by constraining type variables.
fn nullary_fits(t: &mut Typer, ft: TypeId, pt: Pt) -> Result<bool, &'static str> {
    let Pt::Type(p) = pt else { return Ok(true) };
    let before = t.constraint.attempt();
    let fits = t.conforms(ft, p);
    t.constraint.abandon(before);
    match fits {
        true if !t.is_fully_defined(p) => Err(NULLARY_AGAINST_VARS),
        _ => Ok(fits),
    }
}

/// Checks whether `result` refers to a parameter of method type `binder` of `alt`.
pub fn mentions_params(u: &mut Universe, result: TypeId, binder: Binder, alt: SymbolId) -> bool {
    let mut found = false;
    u.map_type(result, &mut |u, part| {
        found |= u.param_ref(part).is_some_and(|(b, _)| b == binder)
            || matches!(*u.ty(part), Type::Sym(None, s)
                if u.symbol(s).kind == Kind::Param && u.symbol(s).owner == Some(alt));
        None
    });
    found
}
