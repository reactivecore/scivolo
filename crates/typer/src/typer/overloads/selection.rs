// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! The alternatives of a qualifier that have the member selected of it (dotty's
//! `SelectionProto.isMatchedBy`).

use super::Resolution;
use super::normalized::normalized;
use crate::typer::{Pt, Typer};
use syntax::names::Name;
use tasty::{SymbolId, Type, TypeId};

/// Why a qualifier is not resolved when its values and its methods taking parameters have the
/// member selected, which dotty tells apart by what is expected of the member.
const MEMBER_OF_BOTH: &str = "overloaded qualifier whose values and methods have the selected member";
/// Why a qualifier is not resolved when an alternative's type is undetermined.
const UNDETERMINED: &str = "overloaded qualifier of an undetermined type";

impl Resolution {
    /// Keeps the alternatives of `alts` whose types, normalized, have term member `name`, as
    /// `f(x: Int): Int` for `f.andThen(g)` beside `def f: Int` (dotty's `isMatchedBy` of a
    /// `SelectionProto`, without comparing the member with what is expected of it); refuses
    /// values beside methods taking parameters that have it, unless it is a member of `Any` or
    /// `Object`, and alternatives of undetermined types.
    pub fn having_member(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        name: Name,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let text = t.text(name).to_owned();
        let (mut kept, mut values, mut methods) = (Vec::new(), false, false);
        for &s in alts {
            let tp = self.alt_type(t, s);
            if !has_member(t, (s, tp), &text)? {
                continue;
            }
            kept.push(s);
            match takes_params(t, tp) {
                true => methods = true,
                false => values = true,
            }
        }
        match values && methods && !is_universal(t, &text) {
            true => Err(MEMBER_OF_BOTH),
            false => Ok(kept),
        }
    }
}

/// Checks whether alternative type `tp` is a method whose first clause takes explicit parameters.
pub fn takes_params(t: &Typer, tp: TypeId) -> bool {
    t.u.method(t.u.strip_poly(tp)).is_some_and(|m| !m.implicit && !m.params.is_empty())
}

/// Checks whether alternative `alt` of type `tp`, normalized, has a term member named `text`: a
/// method with an empty parameter list as a function, else its result (dotty's `normalize`
/// against a `SelectionProto`).
fn has_member(t: &mut Typer, (alt, tp): (SymbolId, TypeId), text: &str) -> Result<bool, &'static str> {
    if has_term_member(t, (alt, tp), text)? {
        return Ok(true);
    }
    match t.u.method(tp) {
        Some(m) if m.params.is_empty() && !m.implicit => has_term_member(t, (alt, m.result), text),
        _ => Ok(false),
    }
}

/// Checks whether type `tp` of `alt`, normalized, has a term member named `text`; refuses an
/// undetermined type variable.
fn has_term_member(t: &mut Typer, (alt, tp): (SymbolId, TypeId), text: &str) -> Result<bool, &'static str> {
    let before = t.constraint.attempt();
    let found = normalized(t, tp, alt, Pt::Any).map(|norm| {
        let norm = t.zonk(norm);
        let members = t.type_members(norm, text);
        let unknown = matches!(t.u.ty(norm), Type::Var(_));
        (unknown, members.into_iter().any(|m| !t.u.is_type_symbol(m)))
    });
    t.constraint.abandon(before);
    match found? {
        (true, _) => Err(UNDETERMINED),
        (false, found) => Ok(found),
    }
}

/// Checks whether `Any`, `AnyRef` or `Object` declares a term named `text`.
fn is_universal(t: &mut Typer, text: &str) -> bool {
    let d = t.defn;
    [d.any, d.any_ref, d.object].into_iter().any(|c| {
        let decls = t.u.decls_named(c, text);
        decls.into_iter().any(|m| !t.u.is_type_symbol(m))
    })
}
