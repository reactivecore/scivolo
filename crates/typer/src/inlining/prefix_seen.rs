//! Types of an expansion seen from the prefixes it substitutes: class type parameters replaced by
//! the arguments the prefix gives them, type members selected anew from it (dotty's name-based
//! `TypeRef`s).

use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The argument the type replacing the `this` of class `C` in `this` gives the type parameter of
/// `C` that `part` refers to, if `this` replaces it and the type gives one.
pub fn class_param_seen(u: &mut Universe, this: &[(SymbolId, TypeId)], part: TypeId) -> Option<TypeId> {
    let (c, _) = u.class_param(part)?;
    if let Some((Some(pre), _)) = u.sym_ref(part)
        && u.this_of(pre).is_none()
    {
        return None;
    }
    let &(_, to) = this.iter().find(|&&(cls, _)| cls == c)?;
    let seen = u.as_seen_from(part, to, c);
    (seen != part && u.class_param(seen).is_none_or(|(o, _)| o != c)).then_some(seen)
}

/// `t` with each type parameter of a class selected from a path replaced by the argument the
/// path's type gives it (dotty's `TypeRef` to a class parameter dereferenced).
pub fn class_params_seen(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let Some((Some(p), _)) = u.sym_ref(part) else { return None };
        if !matches!(u.sym_ref(p), Some((None, s)) if u.symbol(s).kind.is_term()) {
            return None;
        }
        let (c, _) = u.class_param(part)?;
        let seen = u.as_seen_from(part, p, c);
        (seen != part).then_some(seen)
    })
}

/// Whether `t` refers to a static member through the `this` of a static owner or a static
/// object (dotty's `stopBecauseStaticOrLocal` under `StopAt.Static`).
pub fn is_static_ref(u: &mut Universe, t: TypeId) -> bool {
    let Some((Some(p), s)) = u.sym_ref(t) else { return false };
    let owner = match *u.ty(p) {
        Type::This(c) => u.class_symbol(c),
        _ => u.sym_of(p).and_then(|m| u.module_class(m)),
    };
    u.is_static_symbol(s) && owner.is_some_and(|o| u.is_static_owner(o))
}

/// `t` with each abstract type member selected from a path selecting instead the member of that
/// name the path's type defines, if it defines a concrete one.
pub fn reselected_members(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let Some((Some(p), s)) = u.sym_ref(part) else { return None };
        if u.symbol(s).kind != Kind::Type || u.sym_ref(p).is_none() || u.alias_of(s).is_some() {
            return None;
        }
        let name = u.interned_name(s);
        let r = u.resolve(p, name, true).filter(|&r| r != s && u.alias_of(r).is_some())?;
        Some(u.intern_type(Type::Sym(Some(p), r)))
    })
}
