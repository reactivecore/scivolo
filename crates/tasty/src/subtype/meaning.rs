//! What a type symbol or a refinement-only type member stands for through a prefix.

use super::refs::Meaning;
use crate::types::{NameId, Type, TypeId};
use crate::{ANY, Kind, NOTHING, SymbolId, Universe};

impl Universe {
    /// Determines whether private type member `s` is out of reach through `prefix`, where the name
    /// no longer resolves: `this` of a class other than its owner, or a path to an instance of a
    /// subclass of its owner (dotty's "private member in a base class").
    pub fn is_hidden_private(&mut self, s: SymbolId, prefix: TypeId) -> bool {
        let sym = self.symbol(s);
        if sym.kind != Kind::Type || !sym.is_private() {
            return false;
        }
        let Some(owner) = sym.owner else { return false };
        if let Some(this) = self.this_of(prefix) {
            return self.class_symbol(this).is_some_and(|c| c != owner);
        }
        if !self.is_value_path(prefix) {
            return false;
        }
        let widened = self.widen(prefix);
        self.class_symbol(widened).is_some_and(|c| c != owner && self.derives_from(c, owner))
    }
}

/// What type member `name` stands for by a refinement of `prefix`'s type.
pub fn refined_meaning(u: &mut Universe, prefix: TypeId, name: NameId) -> Option<Meaning> {
    let info = u.refinement_named(prefix, name)?;
    match *u.ty(info) {
        Type::Alias(a) => Some(Meaning::Alias(a)),
        Type::Bounds(lo, hi) => Some(Meaning::Abstract(lo, hi)),
        _ => None,
    }
}

/// What class type parameter `p` stands for through value `prefix`: the argument its type's base
/// type gives it (dotty's `argForParam`); `None` for a `this` prefix.
fn class_param_arg(u: &mut Universe, p: SymbolId, prefix: TypeId) -> Option<Meaning> {
    let arg = u.arg_for_param(prefix, p)?;
    Some(u.bounds(arg).map_or(Meaning::Alias(arg), |(lo, hi)| Meaning::Abstract(lo, hi)))
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
            let name = u.interned_name(s);
            let member = u.symbol(s).kind == Kind::Type;
            if member && prefix.is_some_and(|p| u.is_hidden_private(s, p)) {
                return Meaning::Unknown;
            }
            if let Some(refined) = prefix.filter(|_| member).and_then(|p| refined_meaning(u, p, name)) {
                let info = u.info(s);
                return match (refined, *u.ty(info)) {
                    (Meaning::Abstract(lo, hi), Type::Bounds(lo2, hi2)) => {
                        let (lo2, hi2) = (seen(u, lo2), seen(u, hi2));
                        Meaning::Abstract(u.lub(lo, lo2), u.glb(hi, hi2))
                    }
                    (refined, _) => refined,
                };
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
            if let Some(info) = prefix.filter(|_| member).and_then(|p| u.overriding_bounds(p, s))
                && let Type::Bounds(lo, hi) = *u.ty(info)
            {
                return Meaning::Abstract(lo, hi);
            }
            if let Some((lo, hi)) = u.opaque_bounds(s) {
                return Meaning::Abstract(seen(u, lo), seen(u, hi));
            }
            let info = u.info(s);
            match *u.ty(info) {
                Type::Bounds(lo, hi) => Meaning::Abstract(seen(u, lo), seen(u, hi)),
                Type::Lambda { binder, params, result } => {
                    let (lo, hi) = u.bounds(result).unwrap_or((NOTHING, ANY));
                    let lambda = |u: &mut Universe, body| {
                        let body = seen(u, body);
                        u.intern_type(Type::Lambda { binder, params, result: body })
                    };
                    Meaning::Abstract(lambda(u, lo), lambda(u, hi))
                }
                _ => Meaning::Abstract(NOTHING, ANY),
            }
        }
        _ => Meaning::Term(s),
    }
}
