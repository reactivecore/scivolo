// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The prototype selecting among overloaded extractor methods: an application to one argument
//! of the scrutinee's type (dotty's `specialProto` in `trySelectUnapply`).

use super::super::overloads::FunProto;
use super::super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::Tree as T;
use syntax::ast::{Span, Tree};
use tasty::TypeId;

/// Builds the prototype that selects among the alternatives of an overloaded member `name` of
/// `qual`: an application to a value of type `sel`; no prototype for a member without
/// alternatives.
pub fn scrutinee_proto(t: &mut Typer<'_>, qual: TypeId, name: &str, sel: TypeId, ctx: Ctx) -> Pt {
    if t.members_found(qual, name, false).is_none_or(|f| f.syms.len() < 2) {
        return Pt::Any;
    }
    let dummy = t.tpd.node(T::TypeTree { inferred: true }, Span::NONE, Some(sel), None);
    let arg = t.ast.node(Tree::TypeTree);
    t.splices.insert(arg, dummy);
    let proto = t.protos.len();
    let typed = Some(vec![Some(dummy)]);
    t.protos.push(FunProto {
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
