// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Expected types of implicit searches with type variables outside the subset (unbounded ones,
//! ones in unions), or too unspecific to search; variables settled by their bounds.

use super::super::Typer;
use crate::constraint::Constraint;
use tasty::{Type, TypeId, TypeVars, Universe};

/// Whether `tpe` has an uninstantiated type variable without bounds.
pub fn has_unbounded_var(t: &mut Typer<'_>, tpe: TypeId) -> bool {
    let vars = t.open_vars(tpe);
    vars.into_iter().any(|v| t.constraint.bounds(v) == (tasty::NOTHING, tasty::ANY))
}

/// What about the type variables of expected type `tpe` is outside the subset: a union with
/// one, or one without bounds applied as a type constructor.
pub fn unsettled(t: &mut Typer<'_>, tpe: TypeId) -> Option<&'static str> {
    if open_union_within(t, tpe, 16) {
        return Some("implicit search for a union with type variables");
    }
    let unbounded = constructor_vars(t.u, tpe).into_iter().any(|v| {
        let (lo, hi) = t.constraint.bounds(v);
        let hi = match *t.u.types().get(hi) {
            Type::Lambda { result, .. } => result,
            _ => hi,
        };
        lo == tasty::NOTHING && t.is_exactly_any(hi)
    });
    unbounded.then_some("implicit search for a type with an unbounded higher-kinded type variable")
}

/// The uninstantiated type variables of `t` applied as type constructors.
pub fn constructor_vars(u: &mut Universe, t: TypeId) -> Vec<tasty::VarId> {
    let mut constructors = Vec::new();
    u.map_type(t, &mut |u, part| {
        if let Type::Applied(c, _) = *u.types().get(part)
            && let Type::Var(v) = *u.types().get(c)
        {
            constructors.push(v);
        }
        None
    });
    constructors
}

/// Whether one of `vars` is still uninstantiated without lower bound.
pub fn any_open_constructor(c: &Constraint, vars: &[tasty::VarId]) -> bool {
    vars.iter().any(|&v| c.instance(v).is_none() && c.bounds(v).0 == tasty::NOTHING)
}

/// What the search looks for in place of `tpe` (dotty's `wildApprox`): the upper bound of an
/// uninstantiated type variable bounded from above only; that bound applied to the arguments
/// of such a variable applied, if it is a class taking as many; an intersection with such
/// sides with each side so.
pub fn var_upper_bound(t: &mut Typer<'_>, tpe: TypeId) -> Option<TypeId> {
    let tpe = t.zonk(tpe);
    let (var, args) = match t.u.types().get(tpe).clone() {
        Type::Var(v) => (v, None),
        Type::Applied(c, args) => match *t.u.types().get(c) {
            Type::Var(v) => (v, Some(args)),
            _ => return None,
        },
        Type::And(a, b) => {
            let (x, y) = (var_upper_bound(t, a), var_upper_bound(t, b));
            if x.is_none() && y.is_none() {
                return None;
            }
            return Some(t.u.intern_type(Type::And(x.unwrap_or(a), y.unwrap_or(b))));
        }
        _ => return None,
    };
    let (lo, hi) = t.constraint.bounds(var);
    if lo != tasty::NOTHING || t.is_exactly_any(hi) {
        return None;
    }
    let hi = t.zonk(hi);
    let Some(args) = args else { return Some(hi) };
    let class = t.u.class_symbol(hi).filter(|_| matches!(t.u.types().get(hi), Type::Sym(..)))?;
    (t.u.type_params(class).len() == args.len()).then(|| t.u.intern_type(Type::Applied(hi, args)))
}

/// `tpe` with its type variables whose bounds are equal up to aliases instantiated to them, or
/// to their upper bound if their lower bound is not normal and both normalize alike, and so
/// those of the instances.
pub fn determined(t: &mut Typer<'_>, tpe: TypeId) -> TypeId {
    let mut tpe = t.zonk(tpe);
    for _ in 0..4 {
        let mut changed = false;
        for v in t.open_vars(tpe) {
            let (lo, hi) = t.constraint.bounds(v);
            let normal = t.u.normalized(lo);
            if normal != lo && normal == t.u.normalized(hi) {
                t.constraint.instantiate(v, hi);
                changed = true;
            } else if lo == hi || dealiased_parts(t, lo) == dealiased_parts(t, hi) {
                t.constraint.instantiate(v, lo);
                changed = true;
            }
        }
        tpe = t.zonk(tpe);
        if !changed {
            break;
        }
    }
    tpe
}

/// `tpe` zonked with the aliases among its parts dealiased.
fn dealiased_parts(t: &mut Typer<'_>, tpe: TypeId) -> TypeId {
    let tpe = t.zonk(tpe);
    t.u.map_type(tpe, &mut |u, x| {
        let d = u.dealias(x);
        (d != x).then_some(d)
    })
}

/// Whether `tpe` is `Any`, `AnyRef`, or an uninstantiated type variable bounded by them, which
/// no search may answer (dotty's `isUnderspecified`).
pub fn is_underspecified(t: &mut Typer<'_>, tpe: TypeId) -> bool {
    let tpe = t.zonk(tpe);
    match t.u.types().get(tpe).clone() {
        Type::Var(v) => {
            let (_, hi) = t.constraint.bounds(v);
            is_underspecified(t, hi)
        }
        Type::Or(a, b, _) if t.u.class_symbol(b) == Some(t.u.builtins().null) => is_underspecified(t, a),
        Type::Sym(..) | Type::TypeRef(..) => {
            let b = t.u.builtins();
            let tpe = t.u.dealias(tpe);
            let class = match t.u.types().get(tpe).clone() {
                Type::Sym(_, s) => Some(s),
                Type::TypeRef(p, n) => t.u.resolve(p, &n, true),
                _ => None,
            };
            class.is_some_and(|c| c == b.any || c == b.object)
        }
        _ => false,
    }
}

/// Whether `tpe`, its aliases expanded, has a union with an uninstantiated type variable in it.
fn open_union_within(t: &mut Typer<'_>, tpe: TypeId, depth: u32) -> bool {
    let Some(depth) = depth.checked_sub(1) else { return false };
    let tpe = t.u.dealias(tpe);
    match t.u.types().get(tpe).clone() {
        Type::Or(..) => !t.open_vars(tpe).is_empty(),
        Type::And(a, b) => open_union_within(t, a, depth) || open_union_within(t, b, depth),
        Type::Applied(_, args) => args.into_iter().any(|a| open_union_within(t, a, depth)),
        Type::Annotated(a, _) | Type::ByName(a) => open_union_within(t, a, depth),
        Type::Bounds(lo, hi) => open_union_within(t, lo, depth) || open_union_within(t, hi, depth),
        _ => false,
    }
}
