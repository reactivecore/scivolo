// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Which types exclude `null` (dotty's `isNotNull`, `isNullableClass`).

use crate::Lower;
use tasty::tags::{NULLconst, OBJECT};
use tasty::{Kind, SymbolId, Type, TypeId};

/// How many types `is_not_null` looks through.
const MAX_DEPTH: u32 = 64;

/// Whether no value of type `tpe` is null, `explicit_nulls` as under `-Yexplicit-nulls` before
/// erasure (dotty's `isNotNull`).
pub fn is_not_null(l: &mut Lower, tpe: TypeId, explicit_nulls: bool) -> bool {
    not_null(l, tpe, explicit_nulls, MAX_DEPTH)
}

fn not_null(l: &mut Lower, t: TypeId, explicit: bool, depth: u32) -> bool {
    let Some(depth) = depth.checked_sub(1) else { return false };
    let next = |l: &mut Lower, t| not_null(l, t, explicit, depth);
    match l.u.types().get(t).clone() {
        Type::Constant(tag, _) => tag != NULLconst,
        Type::Str(_) => true,
        Type::Flexible(_) => false,
        Type::And(a, b) => next(l, a) || next(l, b),
        Type::Or(a, b, _) => next(l, a) && next(l, b),
        Type::Bounds(_, p)
        | Type::Alias(p)
        | Type::Annotated(p, _)
        | Type::Refined(p, ..)
        | Type::Rec(_, p)
        | Type::ByName(p)
        | Type::Skolem(_, p) => next(l, p),
        Type::Applied(tycon, _) => match l.u.dealias_step(t) {
            d if d != t => next(l, d),
            _ => next(l, tycon),
        },
        Type::TypeRef(..) | Type::Sym(..) if let Some(s) = type_symbol(l, t) => match l.u.symbol(s).kind {
            Kind::Class => !nullable_class(l, s, explicit) && s != l.u.builtins().nothing,
            Kind::Type | Kind::TypeParam => {
                let info = l.u.info(s);
                next(l, info)
            }
            _ => false,
        },
        _ => l.u.widen_once(t).is_some_and(|w| next(l, w)),
    }
}

/// The type symbol named type `t` refers to.
fn type_symbol(l: &mut Lower, t: TypeId) -> Option<SymbolId> {
    let s = match *l.u.types().get(t) {
        Type::Sym(_, s) => s,
        Type::TypeRef(..) => l.u.resolve_ref(t)?,
        _ => return None,
    };
    matches!(l.u.symbol(s).kind, Kind::Class | Kind::Type | Kind::TypeParam).then_some(s)
}

/// Whether class `c` has `null` among its values: `Null`, `Any` or `Matchable` under explicit
/// nulls; else not `Nothing`, a value class or an object.
fn nullable_class(l: &mut Lower, c: SymbolId, explicit: bool) -> bool {
    let builtins = l.u.builtins();
    if explicit {
        return c == builtins.null || c == builtins.any || c == builtins.matchable;
    }
    c != builtins.nothing
        && !l.u.symbol(c).flags.has(OBJECT)
        && !builtins.any_val.is_some_and(|v| l.u.derives_from(c, v))
}
