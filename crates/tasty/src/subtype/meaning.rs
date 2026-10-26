//! What a type symbol or a refinement-only type member stands for through a prefix.

use super::refs::Meaning;
use crate::types::{Type, TypeId};
use crate::{ANY, Kind, NOTHING, SymbolId, Universe};

/// What type member `name` stands for by a refinement of `prefix`'s type.
pub fn refined_meaning(u: &mut Universe, prefix: TypeId, name: &str) -> Option<Meaning> {
    let info = u.refinement_named(prefix, name)?;
    match *u.types().get(info) {
        Type::Alias(a) => Some(Meaning::Alias(a)),
        Type::Bounds(lo, hi) => Some(Meaning::Abstract(lo, hi)),
        _ => None,
    }
}

/// What class type parameter `p` stands for through value `prefix`: the argument its type's base
/// type gives it (dotty's `argForParam`); `None` for a `this` prefix.
fn class_param_arg(u: &mut Universe, p: SymbolId, prefix: TypeId) -> Option<Meaning> {
    if matches!(u.types().get(prefix), Type::This(_)) {
        return None;
    }
    let class = u.symbol(p).owner.filter(|&c| u.symbol(c).kind == Kind::Class)?;
    let index = u.type_params(class).iter().position(|&q| q == p)?;
    let widened = u.widen(prefix);
    let base = u.base_type(widened, class)?;
    let Type::Applied(_, args) = u.types().get(base).clone() else { return None };
    Some(match *u.types().get(*args.get(index)?) {
        Type::Bounds(lo, hi) => Meaning::Abstract(lo, hi),
        _ => Meaning::Alias(args[index]),
    })
}

/// What symbol `s` stands for when referred to through `prefix`.
pub fn meaning(u: &mut Universe, s: SymbolId, prefix: Option<TypeId>) -> Meaning {
    let seen = |u: &mut Universe, t: TypeId| match (prefix, u.symbol(s).owner) {
        (Some(p), Some(owner)) if u.symbol(owner).kind == Kind::Class => u.as_seen_from(t, p, owner),
        _ => t,
    };
    match u.symbol(s).kind {
        Kind::Class => Meaning::Class(s),
        Kind::Type | Kind::TypeParam => {
            let name = u.symbol(s).name.clone();
            let member = u.symbol(s).kind == Kind::Type;
            if let Some(refined) = prefix.filter(|_| member).and_then(|p| refined_meaning(u, p, &name)) {
                return refined;
            }
            if let Some(arg) = prefix.filter(|_| !member).and_then(|p| class_param_arg(u, s, p)) {
                return arg;
            }
            let opaque = u.visible_opaque_alias(s, prefix);
            if let Some(alias) = opaque.or_else(|| u.alias_of(s)) {
                return Meaning::Alias(match (prefix, u.symbol(s).owner) {
                    (Some(p), Some(owner)) if u.symbol(owner).kind == Kind::Class => {
                        u.as_seen_from(alias, p, owner)
                    }
                    _ => alias,
                });
            }
            if let Some(alias) = prefix.filter(|_| member).and_then(|p| u.overriding_alias(p, s)) {
                return Meaning::Alias(alias);
            }
            if let Some((lo, hi)) = u.opaque_bounds(s) {
                return Meaning::Abstract(seen(u, lo), seen(u, hi));
            }
            let info = u.info(s);
            match u.types().get(info).clone() {
                Type::Bounds(lo, hi) => Meaning::Abstract(seen(u, lo), seen(u, hi)),
                Type::Lambda { binder, params, result } => match u.types().get(result).clone() {
                    Type::Bounds(lo, hi) => {
                        let lambda = |u: &mut Universe, body| {
                            let body = seen(u, body);
                            u.intern_type(Type::Lambda { binder, params: params.clone(), result: body })
                        };
                        Meaning::Abstract(lambda(u, lo), lambda(u, hi))
                    }
                    _ => Meaning::Abstract(NOTHING, ANY),
                },
                _ => Meaning::Abstract(NOTHING, ANY),
            }
        }
        _ => Meaning::Term(s),
    }
}
