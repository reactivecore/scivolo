//! The Scala 2 classes of a jar that a class may extend (ADR 0128).

use crate::typer::Typer;
use tasty::SymbolId;

impl Typer<'_> {
    /// Determines why extending parent class `c` is refused for a Scala 2 class of a jar among
    /// its base classes: one with a member not read, or with an abstract method in its class
    /// file that its pickle does not explain, as the getter of a private `val` of a trait.
    pub fn scala2_parent_refusal(&mut self, c: SymbolId) -> Option<String> {
        for b in self.u.linearization(c) {
            if !self.u.is_pickled(b) {
                continue;
            }
            let full = self.u.full_name(b);
            if let Some((m, why)) = self.u.refused_member_of(b) {
                return Some(format!(
                    "extending Scala 2 class `{full}` of a jar: its member `{m}` has {why}"
                ));
            }
            if let Some(m) = self.u.scala2_unexplained_abstract(b) {
                return Some(format!(
                    "extending Scala 2 class `{full}` of a jar: its abstract method `{m}` not read"
                ));
            }
        }
        None
    }
}
