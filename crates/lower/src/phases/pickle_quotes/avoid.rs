// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala

use super::class_bound::class_bound;
use tasty::{ANY, Kind, NOTHING, NameId, SymbolId, Type, TypeId, Universe};
use typer::approx::{self, Approx};

/// `t` with each abstract type that is not static, or reached through a value that is not,
/// approximated by its bounds as its variance asks, an alias through such a value its alias, a
/// member class of a class that is not static its parents, an intersection with `Any` where a
/// part changed its other part (dotty's `AvoidMap` of `getPicklableHoleType`).
pub fn avoided(u: &mut Universe, t: TypeId) -> TypeId {
    avoided_at(u, t, 1).hi()
}

fn avoided_at(u: &mut Universe, t: TypeId, v: i8) -> Approx {
    match u.ty(t).clone() {
        Type::And(a0, b0) => {
            let parts = [avoided_at(u, a0, v), avoided_at(u, b0, v)];
            if parts == [Approx::Exact(a0), Approx::Exact(b0)] {
                return Approx::Exact(t);
            }
            approx::derived(u, &parts, &[], v, &mut |u, p| match (p[0], p[1]) {
                (ANY, b) => b,
                (a, ANY) => a,
                (a, b) => u.intern_type(Type::And(a, b)),
            })
        }
        Type::Or(a, b, soft) => {
            let parts = [avoided_at(u, a, v), avoided_at(u, b, v)];
            approx::derived(u, &parts, &[], v, &mut |u, p| u.intern_type(Type::Or(p[0], p[1], soft)))
        }
        Type::Applied(tycon, args) if u.lambda(tycon).is_none() => avoided_applied(u, t, tycon, &args, v),
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
            approx::refined(u, p, name, i, v)
        }
        Type::Annotated(x, annot) => {
            let parts = [avoided_at(u, x, v)];
            approx::derived(u, &parts, &[], v, &mut |u, p| u.intern_type(Type::Annotated(p[0], annot)))
        }
        Type::Sym(..) | Type::TypeRef(..) => avoided_ref(u, t, v),
        Type::Method { params, result, .. }
        | Type::Poly { params, result, .. }
        | Type::Lambda { params, result, .. } => avoided_lambda(u, t, &params, result, v),
        _ => Approx::Exact(children_avoided(u, t, v)),
    }
}

/// `tycon[args]` avoided at variance `v`, a member class `tycon` to avoid made its class bound
/// applied to `args`.
fn avoided_applied(u: &mut Universe, t: TypeId, tycon: TypeId, args: &[TypeId], v: i8) -> Approx {
    let variances = u.tycon_variances(tycon, args.len());
    let mapped: Vec<Approx> = args
        .iter()
        .zip(&variances)
        .map(|(&a, &pv)| {
            let wildcard = matches!(u.ty(a), Type::Bounds(..) | Type::Wildcard(_));
            avoided_at(u, a, if wildcard { v } else { v * pv })
        })
        .collect();
    if let Some(c) = avoided_class(u, tycon) {
        let args: Vec<TypeId> = mapped.iter().map(|a| a.to_bounds(u)).collect();
        let bound = class_bound(u, c, &args);
        let bound = avoided_at(u, bound, v);
        return approx::range(u, Approx::Exact(NOTHING), bound, v);
    }
    let c = avoided_at(u, tycon, v);
    if c == Approx::Exact(tycon) && mapped.iter().zip(args).all(|(m, &a)| *m == Approx::Exact(a)) {
        return Approx::Exact(t);
    }
    approx::applied(u, c, &mapped, &variances, v)
}

/// Method, polymorphic or lambda type `t` with its parameters avoided contravariantly and its
/// result covariantly (dotty's `derivedLambdaType`).
fn avoided_lambda(u: &mut Universe, t: TypeId, params: &[(NameId, TypeId)], result: TypeId, v: i8) -> Approx {
    let mut parts: Vec<Approx> = params.iter().map(|&(_, p)| avoided_at(u, p, -v)).collect();
    parts.push(avoided_at(u, result, v));
    if parts.iter().zip(params.iter().map(|p| p.1).chain([result])).all(|(m, x)| *m == Approx::Exact(x)) {
        return Approx::Exact(t);
    }
    let flip: Vec<bool> = (0..parts.len()).map(|i| i < params.len()).collect();
    let node = u.ty(t).clone();
    approx::derived(u, &parts, &flip, v, &mut |u, p| {
        let (ps, r) = p.split_at(params.len());
        let ps = params.iter().zip(ps).map(|(&(n, _), &x)| (n, x)).collect();
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

/// Reference `t` avoided at variance `v`.
fn avoided_ref(u: &mut Universe, t: TypeId, v: i8) -> Approx {
    if let Some(c) = avoided_class(u, t) {
        let bound = class_bound(u, c, &[]);
        let bound = avoided_at(u, bound, v);
        return approx::range(u, Approx::Exact(NOTHING), bound, v);
    }
    let (prefix, s) = match *u.ty(t) {
        Type::Sym(prefix, s) => (prefix, Some(s)),
        Type::TypeRef(prefix, _) => (Some(prefix), None),
        _ => return Approx::Exact(t),
    };
    let static_path = match (prefix.map(|p| u.ty(p).clone()), s) {
        (None, Some(s)) => u.is_static_symbol(s),
        (Some(Type::Sym(_, t)), _) => u.symbol(t).kind.is_type() || u.is_static_symbol(t),
        (Some(Type::This(_)), Some(s)) => u.is_static_symbol(s),
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
    match u.bounds(info) {
        Some((lo, hi)) => {
            let (l, h) = (avoided_at(u, lo, -v), avoided_at(u, hi, v));
            approx::range(u, l, h, v)
        }
        None => Approx::Exact(children_avoided(u, t, v)),
    }
}

/// The class `t` refers to if it is a member of a class and not static, or reached through a
/// prefix that is not.
fn avoided_class(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    let s = match *u.ty(t) {
        Type::Sym(_, s) => s,
        Type::TypeRef(..) => u.resolve_ref(t)?,
        _ => return None,
    };
    let owner = u.symbol(s).owner?;
    let member = u.symbol(s).kind == Kind::Class && u.symbol(owner).kind == Kind::Class;
    (member && !static_prefix(u, t)).then_some(s)
}

/// Whether the named types on path `t` are static (dotty's `AvoidMap.isStaticPrefix`).
fn static_prefix(u: &mut Universe, t: TypeId) -> bool {
    let (prefix, s) = match *u.ty(t) {
        Type::Sym(prefix, s) => (prefix, s),
        Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => match u.resolve_ref(t) {
            Some(s) => (Some(prefix), s),
            None => return true,
        },
        _ => return true,
    };
    u.symbol(s).kind == Kind::Package || u.is_static_symbol(s) && prefix.is_none_or(|p| static_prefix(u, p))
}

/// `t` with the types it is made of avoided, each at their upper bound.
fn children_avoided(u: &mut Universe, t: TypeId, v: i8) -> TypeId {
    u.map_type(t, &mut |u, x| (x != t).then(|| avoided_at(u, x, v).hi()))
}
