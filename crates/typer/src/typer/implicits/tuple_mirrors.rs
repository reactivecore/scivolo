// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! `Mirror`s of tuple types (dotty's `makeTupleProductMirror`): `new scala.runtime.TupleMirror(n)`
//! cast to the mirror type; and the tuple types they are made of.

use super::super::Typer;
use super::super::small_tuples::MAX_TUPLE_ARITY;
use super::mirror_types::{MISSING, MirrorCore, MirrorSynthesis, label_pairs};
use crate::tpd::TNodeId;
use syntax::ast::constant::Constant;
use tasty::{Type, TypeId, Universe};

impl MirrorSynthesis {
    /// Synthesizes the mirror of a tuple such as `(Int, String)` with element types `elems`, or of
    /// a named tuple such as `(x: Int, y: Int)` if `names` are given (dotty's
    /// `makeNamedTupleProductMirror`). Refuses a mirror of a tuple type constructor.
    pub fn tuple(
        self,
        t: &mut Typer<'_>,
        elems: &[TypeId],
        names: Option<&[String]>,
    ) -> Result<Option<TNodeId>, &'static str> {
        if matches!(t.u.types().get(self.formal.mirrored), Type::Lambda { .. }) {
            return Err("Mirror of a tuple type lambda");
        }
        let labels: Vec<String> = match names {
            Some(names) => names.to_vec(),
            None => (1..=elems.len()).map(|i| format!("_{i}")).collect(),
        };
        let elem_types = t.nested_pairs(elems)?;
        let elem_labels = label_pairs(t, &labels)?;
        let label = match elems.len() {
            _ if names.is_some() => "NamedTuple".to_owned(),
            n if n <= MAX_TUPLE_ARITY => format!("Tuple{n}"),
            _ => "Tuple".to_owned(),
        };
        let core = MirrorCore {
            parent: "scala.deriving.Mirror$.Product",
            mono: self.formal.mirrored,
            label: &label,
            elems: Some((elem_types, elem_labels)),
        };
        let cast = self.mirror_type(t, core)?;
        let arity = t.build().literal(Constant::Int(elems.len() as i32), self.at);
        let created = self.new_with_args(t, "scala.runtime.TupleMirror", &[arity])?;
        self.cast(t, created, cast)
    }
}

impl Typer<'_> {
    /// Builds the generic tuple type `A *: B *: EmptyTuple` of `elems` (dotty's
    /// `TypeOps.nestedPairs`).
    pub fn nested_pairs(&mut self, elems: &[TypeId]) -> Result<TypeId, &'static str> {
        let pair = self.resolve_full_name("scala.*:", true).ok_or(MISSING)?;
        let empty = self.resolve_full_name("scala.EmptyTuple", false).ok_or(MISSING)?;
        let pair = crate::defn::type_ref(self.u, pair);
        let mut t = self.u.intern_type(Type::Sym(None, empty));
        for &e in elems.iter().rev() {
            t = self.u.intern_type(Type::Applied(pair, vec![e, t]));
        }
        Ok(t)
    }
}

/// Lists the element types of a tuple written with its class, such as `Tuple2[Int, String]` or
/// an alias of it; `None` for another type.
pub fn tuple_class_args(u: &mut Universe, t: TypeId) -> Option<Vec<TypeId>> {
    let d = u.dealias(t);
    let Type::Applied(f, args) = u.types().get(d).clone() else { return None };
    let c = u.class_symbol(f)?;
    (u.full_name(c) == format!("scala.Tuple{}", args.len())).then_some(args)
}
