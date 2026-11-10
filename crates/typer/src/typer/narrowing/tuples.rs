// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! Tuple classes in GADT bounds, which dotty's comparison records as `*:` pairs.

use super::super::Typer;
use tasty::{Type, TypeId, Universe};

/// The arity of `scala.TupleN` if `t` applies it to as many arguments.
fn tuple_arity(u: &mut Universe, t: TypeId) -> Option<usize> {
    let Type::Applied(_, ref args) = *u.types().get(t) else { return None };
    let n = args.len();
    let c = u.class_symbol(t)?;
    let arity: usize = u.full_name(c).strip_prefix("scala.Tuple")?.parse().ok()?;
    (arity == n).then_some(n)
}

impl Typer<'_> {
    /// GADT upper bound `hi` with a tuple class applied as `A *: B *: … *: EmptyTuple.type`
    /// (dotty's `compareAppliedType2` comparing a type with `toNestedPairs` of a `TupleN`).
    pub fn gadt_upper_pairs(&mut self, hi: TypeId) -> TypeId {
        if tuple_arity(self.u, hi).is_none() {
            return hi;
        }
        let Type::Applied(_, args) = self.u.types().get(hi).clone() else { return hi };
        let (Some(pair), Some(empty)) =
            (self.resolve_full_name("scala.*:", true), self.resolve_full_name("scala.EmptyTuple", false))
        else {
            return hi;
        };
        let pair = crate::defn::type_ref(self.u, pair);
        let mut t = self.u.intern_type(Type::Sym(None, empty));
        for &a in args.iter().rev() {
            t = self.u.intern_type(Type::Applied(pair, vec![a, t]));
        }
        t
    }
}
