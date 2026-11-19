// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala

use crate::Lower;
use syntax::ast::Span;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Lower<'_> {
    /// `t`, cast to `tpe` unless its type conforms.
    pub fn ensure_conforms(&mut self, t: TNodeId, tpe: TypeId) -> TNodeId {
        self.cast_unless_conforms(t, tpe, self.defs.as_instance_of)
    }

    /// `t`, cast to `tpe` with `$asInstanceOf$` unless its type conforms (dotty's
    /// `ensureConforms` before erasure).
    pub fn type_cast_unless_conforms(&mut self, t: TNodeId, tpe: TypeId) -> TNodeId {
        self.cast_unless_conforms(t, tpe, self.defs.type_cast)
    }

    fn cast_unless_conforms(&mut self, t: TNodeId, tpe: TypeId, cast: SymbolId) -> TNodeId {
        let from = self.tpd.tpe(t);
        if self.u.is_subtype(from, tpe) {
            return t;
        }
        let fun = self.select(t, cast, Span::NONE);
        let fun = self.positioned(fun);
        let tpt = self.type_tree(tpe, Span::NONE);
        let args = self.tpd.list(&[tpt]);
        let cast = self.tpd.node(Tree::TypeApply { fun, args }, Span::NONE, Some(tpe), Some(cast));
        self.positioned(cast)
    }
}
