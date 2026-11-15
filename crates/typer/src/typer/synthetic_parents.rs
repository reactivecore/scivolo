// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala

use super::Typer;
use crate::defn::{BOXED_UNIT, TUPLE};
use syntax::ast::NodeId;
use tasty::{SymbolId, TypeId};

const JAVA_SERIALIZABLE: &str = "java.io.Serializable";

impl Typer<'_> {
    /// Adjusts the parents `parents` of a library class `cls` defined by `tree` as dotty does when
    /// it completes it: `scala.TupleN[T1, …, Tn]` extends `T1 *: … *: Tn *: EmptyTuple` in place
    /// of `Object`, and `scala.runtime.BoxedUnit` does not extend `java.io.Serializable` (dotty's
    /// `Definitions.adjustForTuple` and `adjustForBoxedUnit`). Refuses a tuple class that extends
    /// a class, and one without `*:` or `EmptyTuple` on the classpath or in the unit.
    pub fn adjust_synthetic_parents(
        &mut self,
        cls: SymbolId,
        tree: NodeId,
        mut parents: Vec<TypeId>,
    ) -> Vec<TypeId> {
        if self.u.full_name(cls) == BOXED_UNIT {
            parents
                .retain(|&t| self.u.class_symbol(t).is_none_or(|c| self.u.full_name(c) != JAVA_SERIALIZABLE));
        }
        if !self.is_tuple_class_symbol(cls) {
            return parents;
        }

        let tparams = self.u.type_params(cls);
        if tparams.is_empty() {
            if let Some(tuple) = self.resolve_full_name(TUPLE, true) {
                parents.push(crate::defn::type_ref(self.u, tuple));
            }
            return parents;
        }

        if parents.first().and_then(|&p| self.u.class_symbol(p)) != Some(self.defn.object) {
            self.unsupported(tree, "a tuple class with a class parent");
            return parents;
        }
        let elems: Vec<TypeId> = tparams.into_iter().map(|p| crate::defn::type_ref(self.u, p)).collect();
        match self.nested_pairs(&elems) {
            Ok(pairs) => parents[0] = pairs,
            Err(what) => {
                self.unsupported(tree, what);
            }
        }
        parents
    }

    /// Whether `cls` is a class `scala.TupleN` (dotty's `Definitions.isTupleClass`).
    fn is_tuple_class_symbol(&self, cls: SymbolId) -> bool {
        let sym = self.u.symbol(cls);
        let digits =
            sym.name.strip_prefix("Tuple").filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
        self.u.is_class(cls) && sym.owner == Some(self.defn.scala) && digits.is_some()
    }
}
