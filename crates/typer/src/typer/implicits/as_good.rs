// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Whether one implicit candidate is as good as another by their types (dotty's `isAsGood` and
//! `isAsGoodValueType` with `preferGeneral`).

use super::super::Typer;
use super::super::type_ops::TypeOps;
use super::alt_types::prepared;
use super::candidates::Candidate;
use crate::ctx::Ctx;
use tasty::{Kind, Type, TypeId};

/// Whether `a` of type `t1` is as good as `b` of type `t2` in `ctx`, the constraint unchanged;
/// `None` if undecided.
pub fn is_as_good(
    t: &mut Typer<'_>,
    a: (Candidate, TypeId),
    b: (Candidate, TypeId),
    ctx: Ctx,
) -> Option<bool> {
    let before = t.constraint.attempt();
    let result = as_good_in(t, a, b, ctx);
    t.constraint.abandon(before);
    result
}

fn as_good_in(
    t: &mut Typer<'_>,
    (a, t1): (Candidate, TypeId),
    (b, t2): (Candidate, TypeId),
    ctx: Ctx,
) -> Option<bool> {
    match t.u.ty(t1).clone() {
        Type::Method { params, .. } => {
            let lambda = matches!(t.u.ty(t2), Type::Method { .. } | Type::Poly { .. } | Type::Lambda { .. });
            if params.is_empty() && lambda {
                return Some(true);
            }
            if t.u.symbol(a.sym).is_extension() && !t.u.symbol(b.sym).is_extension() {
                return Some(false);
            }
            let args: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
            if args.last().is_some_and(|&p| t.repeated_elem(p).is_some()) {
                return None;
            }
            let full_b = t.types().ref_type(b.prefix, b.sym);
            let full_b = t.method_info(full_b).unwrap_or(full_b);
            let full_b = t.poly_instance(full_b, b.sym, false);
            if t.u.method(full_b).is_none() {
                return Some(false);
            }
            t.applicable(b.sym, b.prefix, &args, false, ctx)
        }
        Type::Poly { .. } => {
            let t1 = t.poly_instance(t1, a.sym, true);
            as_good_in(t, (a, t1), (b, t2), ctx)
        }
        _ => match *t.u.ty(t2) {
            Type::Method { .. } => Some(true),
            Type::Poly { result, .. } if t.u.method(result).is_some() => Some(true),
            Type::Poly { .. } => {
                let t2 = t.poly_instance(t2, b.sym, false);
                as_good_value(t, t1, t2, a)
            }
            _ => as_good_value(t, t1, t2, a),
        },
    }
}

/// dotty's `isAsGoodValueType`: an old-style implicit `a` by subtyping with contravariant
/// arguments flipped, a given by the other way round.
fn as_good_value(t: &mut Typer<'_>, t1: TypeId, t2: TypeId, a: Candidate) -> Option<bool> {
    let mut ty = t.types();
    let (p1, p2) = (prepared(&mut ty, t1), prepared(&mut ty, t2));
    if !ty.u.symbol(a.sym).is_old_implicit() {
        return Some(relaxed_conforms(&mut ty, p2, p1));
    }
    let (f1, f2) = (flipped(t, p1, 1)?, flipped(t, p2, 1)?);
    Some(relaxed_conforms(&mut t.types(), f1, f2))
}

/// `a <: b`, or numeric widening between value classes (dotty's `relaxed_<:<`).
pub fn relaxed_conforms(ty: &mut TypeOps<'_>, a: TypeId, b: TypeId) -> bool {
    ty.conforms(a, b) || ty.u.sym_of(b).is_some() && ty.widens_to(a, b)
}

/// `tpe` at variance `v` with each argument of a contravariant parameter in a covariant
/// position replaced by a function from it to `Unit` (dotty's `flip`); `None` where the
/// variance of a type constructor's parameters is unknown.
fn flipped(t: &mut Typer<'_>, tpe: TypeId, v: i32) -> Option<TypeId> {
    if v == 0 {
        return Some(tpe);
    }
    match t.u.ty(tpe).clone() {
        Type::Applied(tycon, args) => {
            let class = match t.u.ty(tycon).clone() {
                Type::Sym(_, s) => Some(s),
                Type::TypeRef(p, n) => t.u.resolve(p, &n, true),
                _ => None,
            };
            let Some(class) = class else {
                let open = matches!(t.u.ty(tycon), Type::Var(_) | Type::Param(..));
                return open.then_some(tpe);
            };
            t.complete(class);
            let params = t.u.type_params(class);
            if params.len() != args.len() {
                return (t.u.symbol(class).kind == Kind::TypeParam).then_some(tpe);
            }
            let mut out = Vec::new();
            for (&arg, &p) in args.iter().zip(&params) {
                let pv = t.u.symbol(p).variance();
                out.push(if v > 0 && pv < 0 {
                    let arg = flipped(t, arg, 1)?;
                    let unit = crate::defn::type_ref(t.u, t.defn.unit);
                    let function = t.function_class(1)?;
                    t.u.intern_type(Type::Applied(function, vec![arg, unit]))
                } else {
                    flipped(t, arg, v * pv)?
                });
            }
            Some(t.u.intern_type(Type::Applied(tycon, out)))
        }
        Type::And(x, y) => {
            let (x, y) = (flipped(t, x, v)?, flipped(t, y, v)?);
            Some(t.u.intern_type(Type::And(x, y)))
        }
        Type::Or(x, y, soft) => {
            let (x, y) = (flipped(t, x, v)?, flipped(t, y, v)?);
            Some(t.u.intern_type(Type::Or(x, y, soft)))
        }
        Type::Annotated(x, annot) => {
            let x = flipped(t, x, v)?;
            Some(t.u.intern_type(Type::Annotated(x, annot)))
        }
        Type::Refined(parent, name, info) => {
            let (parent, info) = (flipped(t, parent, v)?, flipped(t, info, v)?);
            Some(t.u.intern_type(Type::Refined(parent, name, info)))
        }
        Type::Alias(_) => Some(tpe),
        Type::Bounds(lo, hi) => {
            let (lo, hi) = (flipped(t, lo, -v)?, flipped(t, hi, v)?);
            Some(t.u.intern_type(Type::Bounds(lo, hi)))
        }
        _ => {
            let mut applied = false;
            t.u.map_type(tpe, &mut |u, part| {
                applied |= matches!(u.ty(part), Type::Applied(..));
                None
            });
            (!applied).then_some(tpe)
        }
    }
}
