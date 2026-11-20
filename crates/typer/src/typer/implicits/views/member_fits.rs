// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Whether the member a conversion gives applies to the arguments of a call (dotty's
//! `FunProto.isMatchedBy` within `SelectionProto.isMatchedBy`).

use super::super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId};

/// Tells whether a member `name` of `q` applies to arguments of types `args` without converting
/// them, as within an implicit search.
pub fn member_applies(t: &mut Typer<'_>, q: TNodeId, name: &str, args: &[TypeId], ctx: Ctx) -> bool {
    let tpe = t.tpd.tpe(q);
    let members = t.type_members(tpe, name);
    let (applies, _, _) = t.speculatively(|t| {
        members.iter().any(|&s| t.applicable(s, Some(tpe), args, false, ctx) != Some(false))
    });
    applies
}

/// Tells whether a member `name` of `q` applies to arguments of types `args` without converting
/// them, its result conforming to `pt` if it is a method of one clause; `None` if that is unknown.
pub fn member_fits(
    t: &mut Typer<'_>,
    q: TNodeId,
    name: &str,
    args: &[TypeId],
    pt: Option<TypeId>,
    ctx: Ctx,
) -> Option<bool> {
    let tpe = t.tpd.tpe(q);
    let mut known = true;
    for m in t.type_members(tpe, name) {
        if takes_no_arguments(t, tpe, m) {
            continue;
        }
        match t.applicable(m, Some(tpe), args, false, ctx) {
            Some(true) => {}
            Some(false) => continue,
            None => {
                known = false;
                continue;
            }
        }
        let r = t.ref_type(Some(tpe), m);
        let info = t.method_info(r).unwrap_or(r);
        let Some(pt) = pt else { return Some(true) };
        match t.u.method(info).map(|m| m.result) {
            Some(result) if t.u.method(result).is_none() => {
                if t.conforms(result, pt) {
                    return Some(true);
                }
            }
            _ => return Some(true),
        }
    }
    known.then_some(false)
}

/// Tells whether member `m` of `tpe` is a value or parameterless method whose type has no
/// `apply` member, so applies to no arguments.
fn takes_no_arguments(t: &mut Typer<'_>, tpe: TypeId, m: SymbolId) -> bool {
    let r = t.ref_type(Some(tpe), m);
    let info = t.method_info(r).unwrap_or(r);
    let value = match *t.u.ty(info) {
        Type::Method { .. } | Type::Poly { .. } => return false,
        Type::ByName(v) => v,
        _ => info,
    };
    let value = t.u.widen(value);
    t.u.class_symbol(value).is_some() && t.type_members(value, names::nme::APPLY).is_empty()
}
