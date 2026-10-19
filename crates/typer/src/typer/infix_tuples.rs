// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Tuple arguments of infix operations, `a op (b, c)`, passed as separate arguments unless every
//! alternative of `op` is unary (dotty's `needsTupledDual`, `tupledDual`).

use super::Typer;
use syntax::ast::{Attrs, NodeId, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// The elements of the tuple argument of infix application `id`, if it has more than one.
    pub fn infix_tuple_elems(&self, id: NodeId, args: &[NodeId]) -> Option<Vec<NodeId>> {
        if !self.ast.attrs(id).contains(Attrs::INFIX_TUPLE_APPLY) {
            return None;
        }
        let [arg] = *args else { return None };
        let (Tree::Tuple { trees } | Tree::Apply { args: trees, .. }) = *self.ast.tree(arg) else {
            return None;
        };
        Some(self.ast.items(trees).to_vec()).filter(|elems| elems.len() > 1)
    }

    /// Replaces the arguments of `proto` by its tupled dual when not every one of `alts` is unary.
    pub fn untuple_unless_unary(&mut self, proto: usize, alts: &[SymbolId]) -> Result<(), &'static str> {
        let Some(elems) = self.untupled_args(proto, alts) else { return Ok(()) };
        if self.protos[proto].typed.is_some() {
            return Err("infix tuple argument typed before untupling");
        }
        let p = &mut self.protos[proto];
        p.args = elems;
        p.dual = None;
        Ok(())
    }

    /// The tupled dual of the arguments of `proto` if not every one of `alts` is unary.
    pub fn untupled_args(&mut self, proto: usize, alts: &[SymbolId]) -> Option<Vec<NodeId>> {
        let elems = self.protos[proto].dual.clone()?;
        (!alts.iter().all(|&s| self.is_unary(s))).then_some(elems)
    }

    /// Whether the first parameter clause of `s` has exactly one parameter, not repeated
    /// (dotty's `isUnary`).
    fn is_unary(&mut self, s: SymbolId) -> bool {
        self.complete(s);
        let info = self.u.info(s);
        match self.u.param_clauses(info).first().map(|(_, ps)| ps.clone()).as_deref() {
            Some(&[p]) => self.repeated_elem(p).is_none(),
            _ => false,
        }
    }
}
