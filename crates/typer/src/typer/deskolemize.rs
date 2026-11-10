// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Types without skolems (dotty's `deskolemized`, an approximating type map): a skolem by its
//! type where it occurs covariantly, `Nothing` where contravariantly, by bounds where invariantly.

use tasty::{Kind, NOTHING, Type, TypeId, Universe};

/// `t` approximated without skolems at variance `v`; `None` where the approximation needs more
/// than the subset.
pub fn deskolemized(u: &mut Universe, t: TypeId, v: i8) -> Option<TypeId> {
    if !has_skolem(u, t) {
        return Some(t);
    }
    Some(match u.ty(t).clone() {
        Type::Skolem(_, info) => {
            let hi = deskolemized(u, info, 1)?;
            match v {
                1 => hi,
                -1 => NOTHING,
                _ => u.intern_type(Type::Bounds(NOTHING, hi)),
            }
        }
        Type::Sym(Some(p), s) if u.symbol(s).kind == Kind::Class && is_skolem(u, p) => {
            let Type::Skolem(_, info) = *u.ty(p) else { return None };
            let hi = deskolemized(u, info, 1)?;
            let hi = u.intern_type(Type::Sym(Some(hi), s));
            match v {
                1 => hi,
                -1 => NOTHING,
                _ => u.intern_type(Type::Bounds(NOTHING, hi)),
            }
        }
        Type::Sym(Some(p), s) if u.symbol(s).kind == Kind::Type && is_skolem(u, p) => {
            let Type::Skolem(_, info) = *u.ty(p) else { return None };
            let info = deskolemized(u, info, 1)?;
            let member = match u.refinement_info(info, s) {
                Some(refined) => refined,
                None => u.member_info(info, s),
            };
            let bounds = match *u.ty(member) {
                Type::Bounds(lo, hi) => (lo, hi),
                Type::Alias(a) => return deskolemized(u, a, v),
                _ => return None,
            };
            match v {
                1 => u.intern_type(Type::Sym(Some(info), s)),
                -1 => bounds.0,
                _ => u.intern_type(Type::Bounds(bounds.0, bounds.1)),
            }
        }
        Type::Sym(Some(p), s) if u.symbol(s).kind == Kind::TypeParam && is_skolem(u, p) => {
            let Type::Skolem(_, info) = *u.ty(p) else { return None };
            let info = deskolemized(u, info, 1)?;
            let arg = u.arg_for_param(info, s)?;
            let Some((lo, hi)) = u.bounds(arg) else { return deskolemized(u, arg, v) };
            match v {
                1 => hi,
                -1 => lo,
                _ => arg,
            }
        }
        Type::Sym(Some(p), s) if v == 1 => {
            let p = deskolemized(u, p, 1)?;
            match u.refinement_info(p, s).and_then(|i| u.type_alias(i)) {
                Some(a) => a,
                None => u.intern_type(Type::Sym(Some(p), s)),
            }
        }
        Type::TypeRef(p, name) if v == 1 => {
            let p = deskolemized(u, p, 1)?;
            match u.refinement_named(p, name).and_then(|i| u.type_alias(i)) {
                Some(a) => a,
                None => u.intern_type(Type::TypeRef(p, name)),
            }
        }
        Type::TermRef(p, name) if v == 1 => {
            let p = deskolemized(u, p, 1)?;
            u.intern_type(Type::TermRef(p, name))
        }
        _ if v == 0 => return None,
        Type::Applied(tycon, args) => {
            let class = u.class_symbol(tycon)?;
            let variances = u.variances(class);
            let mut mapped = Vec::new();
            for (i, a) in args.into_iter().enumerate() {
                mapped.push(deskolemized(u, a, variances.get(i).copied().unwrap_or(0) * v)?);
            }
            u.intern_type(Type::Applied(tycon, mapped))
        }
        Type::And(a, b) => {
            let (a, b) = (deskolemized(u, a, v)?, deskolemized(u, b, v)?);
            u.intern_type(Type::And(a, b))
        }
        Type::Or(a, b, soft) => {
            let (a, b) = (deskolemized(u, a, v)?, deskolemized(u, b, v)?);
            u.intern_type(Type::Or(a, b, soft))
        }
        _ => return None,
    })
}

/// `t` with each skolem replaced by its type (as dotty's `TreePickler` writes it); `None` unless
/// that is also `t` approximated covariantly.
pub fn skolems_widened(u: &mut Universe, t: TypeId) -> Option<TypeId> {
    let widened = u.map_type(t, &mut |u, x| match *u.ty(x) {
        Type::Skolem(_, info) => Some(info),
        _ => None,
    });
    (deskolemized(u, t, 1)? == widened).then_some(widened)
}

/// Whether `t` has a skolem in it.
pub fn has_skolem(u: &mut Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= is_skolem(u, part);
        None
    });
    found
}

fn is_skolem(u: &Universe, t: TypeId) -> bool {
    matches!(u.ty(t), Type::Skolem(..))
}
