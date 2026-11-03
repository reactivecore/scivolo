// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala

use super::class_bound::class_bound;
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId, Universe};
use typer::approx::{self, Approx};

/// `t` with each abstract type that is not static, or reached through a value that is not, made
/// its bounds, an alias through such a value its alias, a member class of a class that is not
/// static its parents, an intersection with `Any` where a part changed its other part (dotty's
/// `AvoidMap` of `getPicklableHoleType`).
pub fn avoided(u: &mut Universe, t: TypeId) -> TypeId {
    avoid(u, t, 1).hi()
}

fn avoid(u: &mut Universe, t: TypeId, v: i8) -> Approx {
    match u.types().get(t).clone() {
        Type::And(a0, b0) => {
            let parts = [avoid(u, a0, v), avoid(u, b0, v)];
            if parts == [Approx::Exact(a0), Approx::Exact(b0)] {
                return Approx::Exact(t);
            }
            approx::derived(u, &parts, &[], v, &mut |u, p| match (p[0], p[1]) {
                (ANY, b) => b,
                (a, ANY) => a,
                (a, b) => u.intern_type(Type::And(a, b)),
            })
        }
        Type::Applied(tycon, args) => applied(u, t, tycon, &args, v),
        Type::Sym(..) | Type::TypeRef(..) => named(u, t, v).unwrap_or_else(|| Approx::Exact(over(u, t, v))),
        _ => Approx::Exact(over(u, t, v)),
    }
}

/// `t` with its parts avoided.
fn over(u: &mut Universe, t: TypeId, v: i8) -> TypeId {
    u.map_type(t, &mut |u, x| (x != t).then(|| avoid(u, x, v).hi()))
}

/// `tycon[args]` with a member class `tycon` to avoid made its class bound applied to `args`.
fn applied(u: &mut Universe, t: TypeId, tycon: TypeId, args: &[TypeId], v: i8) -> Approx {
    let variances = u.tycon_variances(tycon, args.len());
    let mapped: Vec<Approx> = args
        .iter()
        .zip(&variances)
        .map(|(&a, &pv)| {
            let wildcard = matches!(u.types().get(a), Type::Bounds(..) | Type::Wildcard(_));
            avoid(u, a, if wildcard { v } else { v * pv })
        })
        .collect();
    if let Some(c) = avoided_class(u, tycon) {
        let args: Vec<TypeId> = mapped.iter().map(|a| a.to_bounds(u)).collect();
        let bound = class_bound(u, c, &args);
        let bound = avoid(u, bound, v);
        return approx::range(u, Approx::Exact(NOTHING), bound, v);
    }
    let c = avoid(u, tycon, v);
    if c == Approx::Exact(tycon) && mapped.iter().zip(args).all(|(m, &a)| *m == Approx::Exact(a)) {
        return Approx::Exact(t);
    }
    approx::applied(u, c, &mapped, &variances, v)
}

fn named(u: &mut Universe, t: TypeId, v: i8) -> Option<Approx> {
    if let Some(c) = avoided_class(u, t) {
        let bound = class_bound(u, c, &[]);
        let bound = avoid(u, bound, v);
        return Some(approx::range(u, Approx::Exact(NOTHING), bound, v));
    }
    let (prefix, s) = match *u.types().get(t) {
        Type::Sym(prefix, s) => (prefix, Some(s)),
        Type::TypeRef(prefix, _) => (Some(prefix), None),
        _ => return None,
    };
    let static_path = match (prefix.map(|p| u.types().get(p).clone()), s) {
        (None, Some(s)) => u.is_static_symbol(s),
        (Some(Type::Sym(_, t)), _) => u.symbol(t).kind.is_type() || u.is_static_symbol(t),
        _ => true,
    };
    if static_path {
        return None;
    }
    if prefix.is_some()
        && let Some(alias) = u.referenced_alias(t)
    {
        return Some(avoid(u, alias, v));
    }
    let s = s.filter(|&s| matches!(u.symbol(s).kind, Kind::Type | Kind::TypeParam))?;
    let info = u.info(s);
    let Type::Bounds(lo, hi) = *u.types().get(info) else { return None };
    let (lo, hi) = (avoid(u, lo, -v), avoid(u, hi, v));
    Some(approx::range(u, lo, hi, v))
}

/// The class `t` refers to if it is a member of a class and not static, or reached through a
/// prefix that is not.
fn avoided_class(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    let s = match *u.types().get(t) {
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
    let (prefix, s) = match *u.types().get(t) {
        Type::Sym(prefix, s) => (prefix, s),
        Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => match u.resolve_ref(t) {
            Some(s) => (Some(prefix), s),
            None => return true,
        },
        _ => return true,
    };
    u.symbol(s).kind == Kind::Package || u.is_static_symbol(s) && prefix.is_none_or(|p| static_prefix(u, p))
}
