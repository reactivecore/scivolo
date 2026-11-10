// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/core/NullOpsDecorator.scala
//! Java calls and selections without their `Null` where unsafe nulls are enabled
//! (`adaptToSubType` under `Nullables.unsafeNullsEnabled`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Typer<'_> {
    /// Casts `tree`, a call or selection of a Java member of type `T | Null` or flexible `T`, to
    /// `T` under explicit nulls where unsafe nulls are enabled; `None` otherwise.
    pub fn java_null_stripped(&mut self, tree: TNodeId) -> Option<TNodeId> {
        let mode = self.u.null_mode();
        if !mode.explicit || mode.safe || !matches!(self.tpd.tree(tree), T::Apply { .. } | T::Select { .. }) {
            return None;
        }
        let sym = self.tpd.sym(self.tpd.meth_part(tree))?;
        if !self.is_java_member(sym) {
            return None;
        }
        let tpe = self.tpd.tpe(tree);
        let widened = self.u.widen(tpe);
        let dealiased = self.u.dealias(widened);
        let stripped = match *self.u.ty(dealiased) {
            Type::Flexible(hi) => self.strip_null(hi),
            _ => self.strip_null(tpe),
        };
        if stripped == tpe {
            return None;
        }
        self.cast_to_captured(tree, stripped).ok()
    }
}
