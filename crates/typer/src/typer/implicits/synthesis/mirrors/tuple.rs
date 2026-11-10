// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! `Mirror`s of tuple types (dotty's `makeTupleProductMirror`): `new scala.runtime.TupleMirror(n)`
//! cast to the mirror type.

use super::formal::MirrorCore;
use super::{MirrorSynthesis, label_pairs};
use crate::defn::MIRROR_PRODUCT;
use crate::tpd::TNodeId;
use crate::typer::Typer;
use crate::typer::small_tuples::MAX_TUPLE_ARITY;
use syntax::ast::constant::Constant;
use tasty::{Type, TypeId};

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
            parent: MIRROR_PRODUCT,
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
