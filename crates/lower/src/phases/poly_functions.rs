// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! Polymorphic function values and their `apply` as erasure leaves them: of the function class
//! of their arity.

use super::casts::class_named;
use crate::Lower;
use tasty::{Erased, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The function class of the `apply` of polymorphic function type `t`, or of the one `t` is
/// bounded by; `None` for other types.
pub fn poly_function_class(l: &mut Lower, t: TypeId) -> Option<SymbolId> {
    let info = l.u.poly_function_apply_of(t)?;
    let Erased::Class(name) = l.u.erase_refined_function_apply(info) else { return None };
    class_named(l, &name)
}

/// The `apply` of the function class that `qualifier.apply`, selection `sel` without a symbol,
/// calls on a polymorphic function (dotty's `Erasure.Typer.mapOwner`).
pub fn poly_function_member(l: &mut Lower, sel: TNodeId, qualifier: TNodeId) -> Option<SymbolId> {
    let Tree::Select { name, .. } = *l.tpd.tree(sel) else { return None };
    if l.tpd.sym(sel).is_some() || name.is_type {
        return None;
    }
    let Type::TermRef(_, n) = l.u.types().get(l.tpd.tpe(sel)) else { return None };
    if n != "apply" {
        return None;
    }
    let class = poly_function_class(l, l.tpd.tpe(qualifier))?;
    l.u.decl(class, "apply").unwrap_or_default().first().copied()
}
