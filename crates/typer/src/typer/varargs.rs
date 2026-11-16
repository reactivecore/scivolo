// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala

use super::harmonize::Before;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{SymbolId, TypeId};

/// The repeated parameter of a call, whose arguments become one sequence literal.
pub struct Varargs {
    /// The type of the parameter's elements.
    pub elem: TypeId,
    /// The called method.
    pub meth: Option<SymbolId>,
    /// The call's result type, whose type variables the elements may instantiate.
    pub result: TypeId,
    /// Whether the elements are by-name, `=> T*`.
    pub by_name: bool,
}

impl Typer<'_> {
    /// Packs the typed arguments `elems` of repeated parameter `v`, given after tree `before`,
    /// into a sequence literal (dotty's `makeVarArg`), harmonized if the call's result type is
    /// open, with the constraint reset to `typed`, taken before the first element was typed.
    pub fn packed_varargs(
        &mut self,
        elems: Vec<TNodeId>,
        (before, typed): (TNodeId, Option<Before>),
        v: &Varargs,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let elems = match self.is_open(v.result) {
            true => self.harmonic(elems, Pt::Type(v.elem), typed, ctx)?,
            false => elems,
        };

        let at = self.tpd.span(before).end_pos();
        let elem = self.normalized_tuple_type(v.elem);
        let elem = self.java_object_elem(v.meth, elem);
        let arg = self.seq_literal(&elems, elem, at);
        if let T::Typed { expr, .. } = *self.tpd.tree(arg) {
            if v.by_name {
                self.type_seq_literal(expr);
            }
            self.note_spreads(expr, ctx);
        }
        Ok(arg)
    }
}
