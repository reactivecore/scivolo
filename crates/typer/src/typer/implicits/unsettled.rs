// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Expected types of implicit searches with type variables outside the subset (unbounded ones,
//! ones in unions), or too unspecific to search; variables settled by their bounds.

use super::super::type_ops::TypeOps;
use crate::constraint::Constraint;
use tasty::{Type, TypeId, TypeVars, Universe};

/// Whether `tpe` has an uninstantiated type variable without bounds.
pub fn has_unbounded_var(ty: &mut TypeOps<'_>, tpe: TypeId) -> bool {
    let vars = ty.open_vars(tpe);
    vars.into_iter().any(|v| ty.constraint.bounds(v) == (tasty::NOTHING, tasty::ANY))
}

/// What about the type variables of expected type `tpe` is outside the subset: a union with
/// one, or one without bounds applied as a type constructor.
pub fn unsettled(ty: &mut TypeOps<'_>, tpe: TypeId) -> Option<&'static str> {
    if open_union_within(ty, tpe, 16) {
        return Some("implicit search for a union with type variables");
    }
    let unbounded = constructor_vars(ty.u, tpe).into_iter().any(|v| {
        let (lo, hi) = ty.constraint.bounds(v);
        let hi = ty.u.lambda(hi).map_or(hi, |l| l.result);
        lo == tasty::NOTHING && ty.is_exactly_any(hi)
    });
    unbounded.then_some("implicit search for a type with an unbounded higher-kinded type variable")
}

/// The uninstantiated type variables of `t` applied as type constructors.
pub fn constructor_vars(u: &mut Universe, t: TypeId) -> Vec<tasty::VarId> {
    let mut constructors = Vec::new();
    u.map_type(t, &mut |u, part| {
        if let Some((c, _)) = u.applied(part)
            && let Some(v) = u.var(c)
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
pub fn var_upper_bound(ty: &mut TypeOps<'_>, tpe: TypeId) -> Option<TypeId> {
    let tpe = ty.zonk(tpe);
    let (var, args) = match ty.u.ty(tpe).clone() {
        Type::Var(v) => (v, None),
        Type::Applied(c, args) => (ty.u.var(c)?, Some(args)),
        Type::And(a, b) => {
            let (x, y) = (var_upper_bound(ty, a), var_upper_bound(ty, b));
            if x.is_none() && y.is_none() {
                return None;
            }
            return Some(ty.u.intern_type(Type::And(x.unwrap_or(a), y.unwrap_or(b))));
        }
        _ => return None,
    };
    let (lo, hi) = ty.constraint.bounds(var);
    if lo != tasty::NOTHING || ty.is_exactly_any(hi) {
        return None;
    }
    let hi = ty.zonk(hi);
    let Some(args) = args else { return Some(hi) };
    let class = ty.u.class_symbol(hi).filter(|_| ty.u.sym_of(hi).is_some())?;
    (ty.u.type_params(class).len() == args.len()).then(|| ty.u.intern_type(Type::Applied(hi, args)))
}

/// `tpe` with its type variables whose bounds are equal up to aliases instantiated to them, or
/// to their upper bound if their lower bound is not normal and both normalize alike, and so
/// those of the instances.
pub fn determined(ty: &mut TypeOps<'_>, tpe: TypeId) -> TypeId {
    let mut tpe = ty.zonk(tpe);
    for _ in 0..4 {
        let mut changed = false;
        for v in ty.open_vars(tpe) {
            let (lo, hi) = ty.constraint.bounds(v);
            let normal = ty.u.normalized(lo);
            if normal != lo && normal == ty.u.normalized(hi) {
                ty.constraint.instantiate(v, hi);
                changed = true;
            } else if lo == hi || dealiased_parts(ty, lo) == dealiased_parts(ty, hi) {
                ty.constraint.instantiate(v, lo);
                changed = true;
            }
        }
        tpe = ty.zonk(tpe);
        if !changed {
            break;
        }
    }
    tpe
}

/// `tpe` zonked with the aliases among its parts dealiased.
fn dealiased_parts(ty: &mut TypeOps<'_>, tpe: TypeId) -> TypeId {
    let tpe = ty.zonk(tpe);
    ty.u.map_type(tpe, &mut |u, x| {
        let d = u.dealias(x);
        (d != x).then_some(d)
    })
}

/// Whether `tpe` is `Any`, `AnyRef`, or an uninstantiated type variable bounded by them, which
/// no search may answer (dotty's `isUnderspecified`).
pub fn is_underspecified(ty: &mut TypeOps<'_>, tpe: TypeId) -> bool {
    let tpe = ty.zonk(tpe);
    match ty.u.ty(tpe).clone() {
        Type::Var(v) => {
            let (_, hi) = ty.constraint.bounds(v);
            is_underspecified(ty, hi)
        }
        Type::Or(a, b, _) if ty.u.class_symbol(b) == Some(ty.u.defn().null) => is_underspecified(ty, a),
        Type::Sym(..) | Type::TypeRef(..) => {
            let b = ty.u.defn();
            let tpe = ty.u.dealias(tpe);
            let class = match ty.u.ty(tpe).clone() {
                Type::Sym(_, s) => Some(s),
                Type::TypeRef(p, n) => ty.u.resolve(p, n, true),
                _ => None,
            };
            class.is_some_and(|c| c == b.any || c == b.object)
        }
        _ => false,
    }
}

/// Whether `tpe`, its aliases expanded, has a union with an uninstantiated type variable in it.
fn open_union_within(ty: &mut TypeOps<'_>, tpe: TypeId, depth: u32) -> bool {
    let Some(depth) = depth.checked_sub(1) else { return false };
    let tpe = ty.u.dealias(tpe);
    match ty.u.ty(tpe).clone() {
        Type::Or(..) => !ty.open_vars(tpe).is_empty(),
        Type::And(a, b) => open_union_within(ty, a, depth) || open_union_within(ty, b, depth),
        Type::Applied(_, args) => args.into_iter().any(|a| open_union_within(ty, a, depth)),
        Type::Annotated(a, _) | Type::ByName(a) => open_union_within(ty, a, depth),
        Type::Bounds(lo, hi) => open_union_within(ty, lo, depth) || open_union_within(ty, hi, depth),
        _ => false,
    }
}
