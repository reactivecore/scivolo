// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! The kinds of members dotty's `checkOverride` lets override one another: finality, variables,
//! stability and laziness.

use super::Typer;
use tasty::tags::{FINAL, LAZY, MUTABLE, OBJECT, OVERRIDE};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Why member `d` may not override member `m` of a base class, if it may not.
    pub fn override_kind_refusal(&mut self, d: SymbolId, m: SymbolId) -> Option<&'static str> {
        let (fd, fm) = (self.u.symbol(d).flags, self.u.symbol(m).flags);
        let m_value = self.u.symbol(m).kind != Kind::Def;
        let m_concrete = self.is_concrete(m);
        if fm.has(FINAL) || fm.has(OBJECT) {
            Some("override of a final member")
        } else if fd.has(OVERRIDE) && m_value && fm.has(MUTABLE) {
            Some("override of a mutable variable")
        } else if self.is_stable_member(m) && !self.is_stable_member(d) {
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
}
