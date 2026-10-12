//! Types without skolems (dotty's `deskolemized`, an approximating type map): a skolem by its
//! type where it occurs covariantly, `Nothing` where contravariantly, by bounds where invariantly.

use tasty::{Kind, NOTHING, Type, TypeId, Universe};

/// `t` approximated without skolems at variance `v`; `None` where the approximation needs more
/// than the subset.
pub fn deskolemized(u: &mut Universe, t: TypeId, v: i8) -> Option<TypeId> {
    if !has_skolem(u, t) {
        return Some(t);
    }
    Some(match u.types().get(t).clone() {
        Type::Skolem(_, info) => {
            let hi = deskolemized(u, info, 1)?;
            match v {
                1 => hi,
                -1 => NOTHING,
                _ => u.intern_type(Type::Bounds(NOTHING, hi)),
            }
        }
        Type::Sym(Some(p), s) if u.symbol(s).kind == Kind::Type && is_skolem(u, p) => {
            let Type::Skolem(_, info) = *u.types().get(p) else { return None };
            let info = deskolemized(u, info, 1)?;
            let member = u.member_info(info, s);
            let bounds = match u.types().get(member).clone() {
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
        Type::Or(a, b) => {
            let (a, b) = (deskolemized(u, a, v)?, deskolemized(u, b, v)?);
            u.intern_type(Type::Or(a, b))
        }
        _ => return None,
    })
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
    matches!(u.types().get(t), Type::Skolem(..))
}
