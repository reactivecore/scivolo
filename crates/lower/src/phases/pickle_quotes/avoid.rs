// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala

use tasty::{Kind, Type, TypeId, Universe};

/// `t` with each abstract type that is not static, or reached through a value that is not, made
/// its upper bound, an alias through such a value its alias, an intersection with `Any` where a
/// part changed its other part (dotty's `AvoidMap` of `getPicklableHoleType`).
pub fn avoided(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, x| {
        if let Type::And(a0, b0) = *u.types().get(x) {
            let (a, b) = (avoided(u, a0), avoided(u, b0));
            if (a, b) == (a0, b0) {
                return Some(x);
            }
            return Some(match (a, b) {
                (tasty::ANY, _) => b,
                (_, tasty::ANY) => a,
                _ => u.intern_type(Type::And(a, b)),
            });
        }
        let (prefix, s) = match *u.types().get(x) {
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
            && let Some(alias) = u.referenced_alias(x)
        {
            return Some(avoided(u, alias));
        }
        let s = s.filter(|&s| matches!(u.symbol(s).kind, Kind::Type | Kind::TypeParam))?;
        let info = u.info(s);
        match *u.types().get(info) {
            Type::Bounds(_, hi) => Some(avoided(u, hi)),
            _ => None,
        }
    })
}
