// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Which implicits convert a tree (dotty's `candidateKind` for a view prototype): a method of one
//! explicit parameter, or a value, or a method's result after its implicit clauses, of a class
//! deriving from `Conversion` or `<:<`; and which offer an extension method.

use super::super::super::Typer;
use super::super::candidates::Candidate;
use tasty::{Kind, SymbolId, Type, TypeId};

/// How an implicit converts a tree.
#[derive(Clone, Copy, PartialEq)]
pub enum ViewKind {
    /// A method applied to the tree, as `implicit def toBig(i: Int): BigInt`.
    Method,
    /// A value whose `apply` is applied to the tree, as `given Conversion[Int, BigInt]`.
    Value,
    /// Not a conversion.
    None,
}

/// Determines how candidate `c` converts a tree.
pub fn view_kind(t: &mut Typer<'_>, c: Candidate) -> ViewKind {
    let value = candidate_value(t, c);
    if !t.u.symbol(c.sym).is_implicit() {
        return ViewKind::None;
    }
    match value {
        Err(1) => ViewKind::Method,
        Ok(value) if is_view_type(t, value) => ViewKind::Value,
        _ => ViewKind::None,
    }
}

/// Tells whether candidate `c` gives a value with an extension method `name`, as
/// `given ops: Ops` whose class defines `extension (s: String) def twice`.
pub fn offers_extension(t: &mut Typer<'_>, c: Candidate, name: &str) -> Result<bool, &'static str> {
    let Ok(value) = candidate_value(t, c) else { return Ok(false) };
    Ok(!extension_members(t, value, name)?.is_empty())
}

/// Finds the extension methods `name` of a value of type `value`; fails when the value has
/// another member of that name that is not a method.
pub fn extension_members(
    t: &mut Typer<'_>,
    value: TypeId,
    name: &str,
) -> Result<Vec<SymbolId>, &'static str> {
    let value = t.u.widen(value);
    let members = match t.u.class_symbol(value) {
        Some(k) => t.class_members(k, name),
        None => t.type_members(value, name),
    };
    let extensions: Vec<SymbolId> =
        members.iter().copied().filter(|&e| t.u.symbol(e).is_extension()).collect();
    if !extensions.is_empty() && members.iter().any(|&m| t.u.symbol(m).kind != Kind::Def) {
        return Err("extension method of an implicit value among other members");
    }
    Ok(extensions)
}

/// Tells whether a value of type `v` derives from `Conversion` or `<:<`.
pub fn is_view_type(t: &mut Typer<'_>, v: TypeId) -> bool {
    let v = t.u.widen(v);
    ["scala.Conversion", "scala.<:<"]
        .iter()
        .any(|name| t.resolve_full_name(name, true).is_some_and(|b| t.u.base_type(v, b).is_some()))
}

/// The type of the value candidate `c` gives after its type and implicit parameters, or the
/// number of parameters of its first explicit clause.
fn candidate_value(t: &mut Typer<'_>, c: Candidate) -> Result<TypeId, usize> {
    t.complete(c.sym);
    let tpe = t.ref_type(c.prefix, c.sym);
    let mut info = t.method_info(tpe).unwrap_or(tpe);
    info = t.u.strip_poly(info);
    if let Some(m) = t.u.method(info).filter(|m| !m.implicit) {
        return Err(m.params.len());
    }
    let value = t.strip_implicit(info);
    match *t.u.ty(value) {
        Type::ByName(r) => Ok(r),
        Type::Method { .. } | Type::Poly { .. } => Err(0),
        _ => Ok(value),
    }
}
