//! The Scala 2 classes of a jar that a class may extend (ADR 0128).

use crate::typer::Typer;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Determines why extending parent class `c` is refused for a Scala 2 class of a jar among
    /// its base classes: one with a member not read, or a trait with a concrete `val`, `var` or
    /// `lazy val`, whose initialization by the extending class is not ported.
    pub fn scala2_parent_refusal(&mut self, c: SymbolId) -> Option<String> {
        for b in self.u.linearization(c) {
            if !self.u.is_pickled(b) {
                continue;
            }
            let full = self.u.full_name(b);
            if let Some((m, why)) = self.u.refused_member_of(b) {
                return Some(format!("extending Scala 2 class `{full}` of a jar: its member `{m}` has {why}"));
            }
            let decls = &self.u.symbol(b).decls;
            let value = decls.iter().find(|&&d| self.u.symbol(d).kind == Kind::Val && !self.u.is_deferred(d));
            if let Some(&v) = value.filter(|_| self.u.is_trait(b)) {
                let name = &self.u.symbol(v).name;
                return Some(format!("extending Scala 2 trait `{full}` of a jar with the value `{name}`"));
            }
        }
        None
    }
}
