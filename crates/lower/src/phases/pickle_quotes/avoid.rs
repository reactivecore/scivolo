// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala

use tasty::{Kind, Type, TypeId, Universe};
use typer::approx::{self, Approx};

/// `t` with each abstract type that is not static, or reached through a value that is not,
/// approximated by its bounds as its variance asks, an alias through such a value its alias, an
/// intersection with `Any` where a part changed its other part (dotty's `AvoidMap` of
/// `getPicklableHoleType`).
pub fn avoided(u: &mut Universe, t: TypeId) -> TypeId {
    avoided_at(u, t, 1).hi()
}

fn avoided_at(u: &mut Universe, t: TypeId, v: i8) -> Approx {
    match u.types().get(t).clone() {
        Type::And(a0, b0) => {
            let parts = [avoided_at(u, a0, v), avoided_at(u, b0, v)];
            if parts == [Approx::Exact(a0), Approx::Exact(b0)] {
                return Approx::Exact(t);
            }
            approx::derived(u, &parts, &[], v, &mut |u, p| match (p[0], p[1]) {
                (tasty::ANY, b) => b,
                (a, tasty::ANY) => a,
                (a, b) => u.intern_type(Type::And(a, b)),
            })
        }
        Type::Or(a, b, soft) => {
            let parts = [avoided_at(u, a, v), avoided_at(u, b, v)];
            approx::derived(u, &parts, &[], v, &mut |u, p| u.intern_type(Type::Or(p[0], p[1], soft)))
        }
        Type::Applied(tycon, args) if !matches!(u.types().get(tycon), Type::Lambda { .. }) => {
            let c = avoided_at(u, tycon, v);
            let variances = approx::tycon_variances(u, tycon, args.len());
            let mapped: Vec<Approx> = args
                .iter()
                .zip(&variances)
                .map(|(&a, &pv)| {
                    let wildcard = matches!(u.types().get(a), Type::Bounds(..) | Type::Wildcard(_));
                    avoided_at(u, a, if wildcard { v } else { v * pv })
                })
                .collect();
            if c == Approx::Exact(tycon) && mapped.iter().zip(&args).all(|(m, &a)| *m == Approx::Exact(a)) {
                return Approx::Exact(t);
            }
            approx::applied(u, c, &mapped, &variances, v)
        }
        Type::Bounds(lo, hi) => {
            let (l, h) = (avoided_at(u, lo, -v), avoided_at(u, hi, v));
            approx::bounds(u, l, h, v)
        }
        Type::Alias(x) => {
            let x = avoided_at(u, x, 0);
            approx::alias(u, x, v)
        }
        Type::Refined(parent, name, info) => {
            let (p, i) = (avoided_at(u, parent, v), avoided_at(u, info, v));
            if (p, i) == (Approx::Exact(parent), Approx::Exact(info)) {
                return Approx::Exact(t);
            }
            approx::refined(u, p, &name, i, v)
        }
        Type::Annotated(x, annot) => {
            let parts = [avoided_at(u, x, v)];
            approx::derived(u, &parts, &[], v, &mut |u, p| u.intern_type(Type::Annotated(p[0], annot)))
        }
        Type::Sym(prefix, s) => avoided_ref(u, t, prefix, Some(s), v),
        Type::TypeRef(prefix, _) => avoided_ref(u, t, Some(prefix), None, v),
        Type::Method { params, result, .. }
        | Type::Poly { params, result, .. }
        | Type::Lambda { params, result, .. } => avoided_lambda(u, t, &params, result, v),
        _ => Approx::Exact(children_avoided(u, t, v)),
    }
}

/// Method, polymorphic or lambda type `t` with its parameters avoided contravariantly and its
/// result covariantly (dotty's `derivedLambdaType`).
fn avoided_lambda(u: &mut Universe, t: TypeId, params: &[(String, TypeId)], result: TypeId, v: i8) -> Approx {
    let mut parts: Vec<Approx> = params.iter().map(|&(_, p)| avoided_at(u, p, -v)).collect();
    parts.push(avoided_at(u, result, v));
    if parts.iter().zip(params.iter().map(|p| p.1).chain([result])).all(|(m, x)| *m == Approx::Exact(x)) {
        return Approx::Exact(t);
    }
    let flip: Vec<bool> = (0..parts.len()).map(|i| i < params.len()).collect();
    let node = u.types().get(t).clone();
    approx::derived(u, &parts, &flip, v, &mut |u, p| {
        let (ps, r) = p.split_at(params.len());
        let ps = params.iter().zip(ps).map(|((n, _), &x)| (n.clone(), x)).collect();
        u.intern_type(match node.clone() {
            Type::Method { binder, implicit, contextual, .. } => {
                Type::Method { binder, params: ps, result: r[0], implicit, contextual }
            }
            Type::Poly { binder, .. } => Type::Poly { binder, params: ps, result: r[0] },
            Type::Lambda { binder, .. } => Type::Lambda { binder, params: ps, result: r[0] },
            other => other,
        })
    })
}

/// Reference `t` to `s` or through `prefix` avoided at variance `v`.
fn avoided_ref(
    u: &mut Universe,
    t: TypeId,
    prefix: Option<TypeId>,
    s: Option<tasty::SymbolId>,
    v: i8,
) -> Approx {
    let static_path = match (prefix.map(|p| u.types().get(p).clone()), s) {
        (None, Some(s)) => u.is_static_symbol(s),
        (Some(Type::Sym(_, t)), _) => u.symbol(t).kind.is_type() || u.is_static_symbol(t),
        _ => true,
    };
    if static_path {
        return Approx::Exact(t);
    }
    if prefix.is_some()
        && let Some(alias) = u.referenced_alias(t)
    {
        return avoided_at(u, alias, v);
    }
    let Some(s) = s.filter(|&s| matches!(u.symbol(s).kind, Kind::Type | Kind::TypeParam)) else {
        return Approx::Exact(children_avoided(u, t, v));
    };
    let info = u.info(s);
    match *u.types().get(info) {
        Type::Bounds(lo, hi) => {
            let (l, h) = (avoided_at(u, lo, -v), avoided_at(u, hi, v));
            approx::range(u, l, h, v)
        }
        _ => Approx::Exact(children_avoided(u, t, v)),
    }
}

/// `t` with the types it is made of avoided, each at their upper bound.
fn children_avoided(u: &mut Universe, t: TypeId, v: i8) -> TypeId {
    u.map_type(t, &mut |u, x| (x != t).then(|| avoided_at(u, x, v).hi()))
}
