// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Whether an alternative applies to arguments of given types (dotty's `isApplicableMethodRef`),
//! and the parameter types it is compared by.

use super::compatible::compatible;
use super::poly_instances::poly_instance;
use crate::ctx::Ctx;
use crate::typer::Typer;
use syntax::config::Features;
use tasty::{SymbolId, Type, TypeId, Universe};

/// Checks whether arguments of types `args`, their wildcards captured if need be, fit the first
/// parameter clause of `alt` seen from `prefix` in `ctx`, through implicit conversions if
/// `views`: `None` if undecided.
pub fn applicable(
    t: &mut Typer,
    alt: SymbolId,
    prefix: Option<TypeId>,
    args: &[TypeId],
    views: bool,
    ctx: Ctx,
) -> Option<bool> {
    let slots: Vec<Option<TypeId>> = args.iter().copied().map(Some).collect();
    applicable_slots(t, alt, prefix, &slots, views, ctx)
}

/// Checks `applicable` for arguments aligned with the parameters, `None` where a parameter gets
/// none and takes its default or an empty vararg, as `f(y = 1)` of `def f(x: Int = 0, y: Int)`;
/// the constraint unchanged.
pub fn applicable_slots(
    t: &mut Typer,
    alt: SymbolId,
    prefix: Option<TypeId>,
    args: &[Option<TypeId>],
    views: bool,
    ctx: Ctx,
) -> Option<bool> {
    let before = t.constraint.attempt();
    let fits = applicable_with_vars(t, alt, prefix, args, views, ctx);
    t.constraint.abandon(before);
    fits
}

/// Checks `applicable_slots`, constraining type variables.
fn applicable_with_vars(
    t: &mut Typer,
    alt: SymbolId,
    prefix: Option<TypeId>,
    args: &[Option<TypeId>],
    views: bool,
    ctx: Ctx,
) -> Option<bool> {
    let (mut params, repeated) = param_types(t, alt, prefix, true)?;
    let elem = repeated.then(|| params.pop()).flatten();
    if args.len() > params.len() && elem.is_none() {
        return Some(false);
    }
    let missing = (0..params.len()).filter(|&i| args.get(i).is_none_or(Option::is_none)).collect::<Vec<_>>();
    if !missing.is_empty() {
        let defaults = t.default_params(alt);
        if !missing.iter().all(|&i| defaults.get(i) == Some(&true)) {
            return Some(false);
        }
    }

    let mut all = true;
    for (i, &a) in args.iter().enumerate() {
        let Some(a) = a else { continue };
        if let Some(e) = t.repeated_elem(a) {
            let placed = i == params.len() && i + 1 == args.len()
                || i >= params.len()
                    && t.feature_enabled(Features::MULTI_SPREADS, "experimental.multiSpreads", ctx);
            all &= placed && elem.is_some_and(|p| t.conforms(e, p));
            continue;
        }
        let p = params.get(i).copied().or(elem)?;
        let p = t.from_java_object(Some(alt), p);
        let (a, p) = (by_name_result(t.u, a), by_name_result(t.u, p));
        let fits = compatible(t, a, p, views, ctx)?;
        all &= fits || t.captured_args(a).is_some_and(|c| t.conforms(c, p));
    }
    Some(all)
}

/// Gives the result type of by-name type `x`, else `x`.
fn by_name_result(u: &Universe, x: TypeId) -> TypeId {
    match *u.types().get(x) {
        Type::ByName(r) => r,
        _ => x,
    }
}

/// Finds `param_types` of `alt` with its type parameters, if any, replaced by fresh abstract
/// types within their bounds (dotty's `newTypeParams` in `isAsGood`).
pub fn rigid_param_types(
    t: &mut Typer,
    alt: SymbolId,
    prefix: Option<TypeId>,
) -> Option<(Vec<TypeId>, bool)> {
    let tpe = t.u.intern_type(Type::Sym(prefix, alt));
    let info = t.method_info(tpe)?;
    if !matches!(t.u.types().get(info), Type::Poly { .. }) {
        return param_types(t, alt, prefix, false);
    }
    let info = poly_instance(t, info, alt, true);
    first_clause(t, info)
}

/// Finds the parameter types of the first clause of `alt` seen from `prefix`, a repeated one as
/// its element type, and whether the last is repeated; of a polymorphic `alt` only if `poly`,
/// its type parameters replaced by fresh type variables within their bounds.
pub fn param_types(
    t: &mut Typer,
    alt: SymbolId,
    prefix: Option<TypeId>,
    poly: bool,
) -> Option<(Vec<TypeId>, bool)> {
    let tpe = t.u.intern_type(Type::Sym(prefix, alt));
    let mut info = t.method_info(tpe)?;
    if matches!(t.u.types().get(info), Type::Poly { .. }) {
        if !poly {
            return None;
        }
        info = poly_instance(t, info, alt, false);
    }
    first_clause(t, info)
}

/// Finds the parameter types of method type `info`, a repeated last one as its element type,
/// and whether it is repeated.
fn first_clause(t: &mut Typer, info: TypeId) -> Option<(Vec<TypeId>, bool)> {
    let mut types = t.u.method(info)?.param_types();
    let elem = types.last().and_then(|&p| t.repeated_elem(p));
    if let Some(e) = elem {
        *types.last_mut().expect("a parameter") = e;
    }
    Some((types, elem.is_some()))
}
