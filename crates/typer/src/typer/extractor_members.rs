// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The members of an extractor's result that patterns match against (dotty's
//! `extractorMemberType`, `productSelectorTypes`, `getUnapplySelectors`,
//! `unapplySeqTypeElemTp`).

use super::Typer;
use crate::defn::{BOOLEAN, INT};
use tasty::{Binding, Kind, Type, TypeId};

/// The class of generic tuples `A *: B *: EmptyTuple`, whose elements patterns match.
pub const NON_EMPTY_TUPLE: &str = "scala.NonEmptyTuple";
/// The class of sequences, whose elements the patterns of an `unapplySeq` match.
pub const SEQ: &str = "scala.collection.Seq";

/// Finds the types of the product selectors `_1`, `_2`, ... of `t`, as `Int, String` of a
/// `(Int, String)` or of `case class C(i: Int, s: String)`. Refuses an overloaded selector.
pub fn product_selectors(t: &mut Typer<'_>, tpe: TypeId) -> Result<Vec<TypeId>, &'static str> {
    let mut out = Vec::new();
    while let Some(s) = extractor_member(t, tpe, &format!("_{}", out.len() + 1))? {
        out.push(s);
    }
    Ok(out)
}

/// Finds the type of `get` if `tpe` also has a `Boolean` member `isEmpty`, as `T` of an
/// `Option[T]`.
pub fn get_match(t: &mut Typer<'_>, tpe: TypeId) -> Result<Option<TypeId>, &'static str> {
    let empty = extractor_member(t, tpe, "isEmpty")?;
    if !empty.is_some_and(|e| t.derives_from_named(e, BOOLEAN)) {
        return Ok(None);
    }
    extractor_member(t, tpe, "get")
}

/// Finds the type of parameterless member `name` of `tpe` (dotty's `extractorMemberType`).
/// Refuses an overloaded member and one whose type is a wildcard.
pub fn extractor_member(t: &mut Typer<'_>, tpe: TypeId, name: &str) -> Result<Option<TypeId>, &'static str> {
    let Some(found) = t.members_found(tpe, name, false) else { return Ok(None) };
    let [sym] = found.syms[..] else { return Err("overloaded extractor member") };
    let member = t.ref_type(Some(tpe), sym);
    let info = t.method_info(member).unwrap_or(member);
    Ok(match *t.u.ty(info) {
        Type::Method { .. } | Type::Poly { .. } => None,
        Type::Bounds(..) => return Err("extractor member typed by a wildcard argument"),
        Type::ByName(r) => Some(r),
        _ => Some(info),
    })
}

/// Finds the element type of sequence-like `tpe`, the result of its `apply(Int)`, if it also
/// has `lengthCompare(Int): Int` or `length: Int`, and `drop(Int)` and `toSeq` that are
/// `collection.Seq`s of it, as `T` of a `List[T]` (dotty's `unapplySeqTypeElemTp`).
pub fn seq_elem_type(t: &mut Typer<'_>, tpe: TypeId) -> Option<TypeId> {
    let elem = int_method_results(t, tpe, "apply").into_iter().next()?;
    let seq_class = t.resolve_full_name(SEQ, true)?;
    let seq_class = crate::defn::type_ref(t.u, seq_class);
    let seq = t.u.intern_type(Type::Applied(seq_class, vec![elem]));

    let length = int_method_results(t, tpe, "lengthCompare").into_iter().any(|r| is_int(t, r))
        || parameterless_results(t, tpe, "length").into_iter().any(|r| is_int(t, r));
    let drop = int_method_results(t, tpe, "drop").into_iter().any(|r| t.u.is_subtype(r, seq));
    let to_seq = parameterless_results(t, tpe, "toSeq").into_iter().any(|r| t.u.is_subtype(r, seq));
    (length && drop && to_seq).then_some(elem)
}

/// The results of the alternatives of method `name` of `tpe` that take one `Int`.
fn int_method_results(t: &mut Typer<'_>, tpe: TypeId, name: &str) -> Vec<TypeId> {
    let mut out = Vec::new();
    for s in t.members_found(tpe, name, false).map(|f| f.syms).unwrap_or_default() {
        let member = t.ref_type(Some(tpe), s);
        let Some(info) = t.method_info(member) else { continue };
        let Some(Binding { params, result, .. }) = t.u.method(info) else { continue };
        if let [(_, index)] = params[..]
            && is_int(t, index)
            && !matches!(t.u.ty(result), Type::Method { .. } | Type::Poly { .. })
        {
            out.push(result);
        }
    }
    out
}

fn is_int(t: &mut Typer<'_>, tpe: TypeId) -> bool {
    t.u.is_class_named(tpe, INT)
}

/// The results of the alternatives of member `name` of `tpe` that are parameterless methods.
fn parameterless_results(t: &mut Typer<'_>, tpe: TypeId, name: &str) -> Vec<TypeId> {
    let mut out = Vec::new();
    for s in t.members_found(tpe, name, false).map(|f| f.syms).unwrap_or_default() {
        if t.u.symbol(s).kind != Kind::Def {
            continue;
        }
        let member = t.ref_type(Some(tpe), s);
        let info = t.method_info(member).unwrap_or(member);
        match *t.u.ty(info) {
            Type::Method { .. } | Type::Poly { .. } => {}
            Type::ByName(r) => out.push(r),
            _ => out.push(info),
        }
    }
    out
}
