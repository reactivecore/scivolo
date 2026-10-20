// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! Bounds without the parameters of the type lambdas being compared (dotty's
//! `avoidLambdaParams`).

use crate::typer::approx::{self, Approx};
use tasty::{ANY, NOTHING, Type, TypeId, Universe};

/// Bound `t` of a variable, a lower one if `from_below`, with each parameter of the compared
/// type lambdas approximated by `Nothing .. Any`; `None` where such a parameter remains.
pub fn avoid_lambda_params(u: &mut Universe, t: TypeId, from_below: bool) -> Option<TypeId> {
    if !refers_to_compared(u, t) {
        return Some(t);
    }
    let v = if from_below { 1 } else { -1 };
    let mapped = approx_at(u, t, v)?;
    (!refers_to_compared(u, mapped.hi())).then_some(mapped.hi())
}

fn refers_to_compared(u: &mut Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, x| {
        found |= matches!(u.types().get(x), Type::Param(b, _) if b.0 == Universe::COMPARED_BINDERS);
        None
    });
    found
}

fn approx_at(u: &mut Universe, t: TypeId, v: i8) -> Option<Approx> {
    if !refers_to_compared(u, t) {
        return Some(Approx::Exact(t));
    }
    match u.types().get(t).clone() {
        Type::Param(..) => Some(approx::range(u, Approx::Exact(NOTHING), Approx::Exact(ANY), v)),
        Type::Applied(tycon, args) => {
            let c = approx_at(u, tycon, v)?;
            let variances = approx::tycon_variances(u, tycon, args.len());
            let mut mapped = Vec::with_capacity(args.len());
            for (&arg, &pv) in args.iter().zip(&variances) {
                let wildcard = matches!(u.types().get(arg), Type::Bounds(..) | Type::Wildcard(_));
                mapped.push(approx_at(u, arg, if wildcard { v } else { v * pv })?);
            }
            Some(approx::applied(u, c, &mapped, &variances, v))
        }
        Type::Bounds(lo, hi) => {
            let (lo, hi) = (approx_at(u, lo, -v)?, approx_at(u, hi, v)?);
            Some(approx::bounds(u, lo, hi, v))
        }
        Type::And(x, y) => {
            let parts = [approx_at(u, x, v)?, approx_at(u, y, v)?];
            Some(approx::derived(u, &parts, &[], v, &mut |u, p| u.intern_type(Type::And(p[0], p[1]))))
        }
        Type::Or(x, y, soft) => {
            let parts = [approx_at(u, x, v)?, approx_at(u, y, v)?];
            Some(approx::derived(u, &parts, &[], v, &mut |u, p| u.intern_type(Type::Or(p[0], p[1], soft))))
        }
        _ => None,
    }
}
