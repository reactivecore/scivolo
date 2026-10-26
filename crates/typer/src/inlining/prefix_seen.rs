//! Types of an expansion seen from the prefixes it substitutes: class type parameters replaced by
//! the arguments the prefix gives them, type members selected anew from it (dotty's name-based
//! `TypeRef`s).

use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The argument the type replacing the `this` of class `C` in `this` gives the type parameter of
/// `C` that `part` refers to, if `this` replaces it and the type gives one.
pub fn class_param_seen(u: &mut Universe, this: &[(SymbolId, TypeId)], part: TypeId) -> Option<TypeId> {
    let (c, _) = u.class_param(part)?;
    if let Type::Sym(Some(pre), _) = *u.types().get(part)
        && !matches!(u.types().get(pre), Type::This(_))
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
        let Type::Sym(Some(p), _) = *u.types().get(part) else { return None };
        if !matches!(u.types().get(p), Type::Sym(None, s) if u.symbol(*s).kind.is_term()) {
            return None;
        }
        let (c, _) = u.class_param(part)?;
        let seen = u.as_seen_from(part, p, c);
        (seen != part).then_some(seen)
    })
}

/// `t` with each abstract type member selected from a path selecting instead the member of that
/// name the path's type defines, if it defines a concrete one.
pub fn reselected_members(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let Type::Sym(Some(p), s) = *u.types().get(part) else { return None };
        if u.symbol(s).kind != Kind::Type
            || !matches!(u.types().get(p), Type::Sym(..))
            || u.alias_of(s).is_some()
        {
            return None;
        }
        let name = u.symbol(s).name.clone();
        let r = u.resolve(p, &name, true).filter(|&r| r != s && u.alias_of(r).is_some())?;
        Some(u.intern_type(Type::Sym(Some(p), r)))
    })
}
