// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! Tuple classes in GADT bounds, which dotty's comparison records as `*:` pairs.

use super::super::Typer;
use tasty::{Type, TypeId, Universe};

/// Gives the arity of `scala.TupleN` if `t` applies it to as many arguments.
fn tuple_arity(u: &mut Universe, t: TypeId) -> Option<usize> {
    let n = u.applied(t)?.1.len();
    let c = u.class_symbol(t)?;
    let arity: usize = u.full_name(c).strip_prefix(crate::defn::TUPLE)?.parse().ok()?;
    (arity == n).then_some(n)
}

/// Rewrites GADT upper bound `hi` that applies a tuple class as `A *: B *: … *: EmptyTuple.type`
/// (dotty's `compareAppliedType2` comparing a type with `toNestedPairs` of a `TupleN`).
pub fn upper_pairs(t: &mut Typer, hi: TypeId) -> TypeId {
    if tuple_arity(t.u, hi).is_none() {
        return hi;
    }
    let Some(args) = t.u.applied(hi).map(|(_, args)| args.to_vec()) else { return hi };
    let (Some(pair), Some(empty)) =
        (t.resolve_full_name("scala.*:", true), t.resolve_full_name("scala.EmptyTuple", false))
    else {
        return hi;
    };
    let pair = crate::defn::type_ref(t.u, pair);
    let mut ty = t.u.intern_type(Type::Sym(None, empty));
    for &a in args.iter().rev() {
        ty = t.u.intern_type(Type::Applied(pair, vec![a, ty]));
    }
    ty
}
