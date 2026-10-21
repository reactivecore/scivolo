// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The prototype selecting among overloaded extractor methods: an application to one argument
//! of the scrutinee's type (dotty's `specialProto` in `trySelectUnapply`).

use super::overloads::FunProto;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::Tree as T;
use syntax::ast::{Span, Tree};
use tasty::TypeId;

impl Typer<'_> {
    /// For member `name` of `qual` with several alternatives, an application to a value of type
    /// `sel`, else `Pt::Any`.
    pub fn scrutinee_proto(&mut self, qual: TypeId, name: String, sel: TypeId, ctx: Ctx) -> Pt {
        if self.members_found(qual, &name, false).is_none_or(|f| f.syms.len() < 2) {
            return Pt::Any;
        }
        let dummy = self.tpd.node(T::TypeTree { inferred: true }, Span::NONE, Some(sel), None);
        let arg = self.ast.node(Tree::TypeTree);
        self.splices.insert(arg, dummy);
        let proto = self.protos.len();
        let typed = Some(vec![Some(dummy)]);
        self.protos.push(FunProto {
            args: vec![arg],
            ctx,
            typed,
            pt: Pt::Any,
            deep: false,
            dual: None,
            using: false,
        });
        Pt::Fun(proto)
    }
}
