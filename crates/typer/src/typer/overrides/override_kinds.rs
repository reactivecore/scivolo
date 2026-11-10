// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! The kinds of members dotty's `checkOverride` lets override one another: finality, variables,
//! stability and laziness.

use super::super::Typer;
use super::super::stability::is_stable_member;
use tasty::tags::{FINAL, LAZY, MUTABLE, OBJECT, OVERRIDE};
use tasty::{Kind, SymbolId};

/// Why member `d` may not override member `m` of a base class, if it may not: `m` is final or an
/// overridden `var`, or `d` is a `def` or `var` overriding a `val`, an `object` overriding a
/// concrete `val`, or a `lazy val` overriding a `val` or the other way round.
pub fn kind_refusal(t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> Option<&'static str> {
    let (fd, fm) = (t.u.symbol(d).flags, t.u.symbol(m).flags);
    let m_value = t.u.symbol(m).kind != Kind::Def;
    let m_concrete = t.is_concrete(m);
    if fm.has(FINAL) || fm.has(OBJECT) {
        Some("override of a final member")
    } else if fd.has(OVERRIDE) && m_value && fm.has(MUTABLE) {
        Some("override of a mutable variable")
    } else if is_stable_member(t.u, m) && !is_stable_member(t.u, d) {
        Some("unstable member overriding a stable value")
    } else if fd.has(OBJECT) && m_value && m_concrete && !fm.has(LAZY) {
        Some("object overriding a concrete non-lazy value")
    } else if fd.has(LAZY) && m_value && !fm.has(LAZY) {
        Some("lazy value overriding a non-lazy value")
    } else if fm.has(LAZY) && m_value && !fd.has(LAZY) && !fd.has(OBJECT) {
        Some("non-lazy value overriding a lazy value")
    } else {
        None
    }
}
