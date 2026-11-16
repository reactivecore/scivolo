// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Whether code may name a class with an access modifier (dotty's `isAccessibleFrom` of a class).

use super::{Kind, SymbolId, Universe};
use crate::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};

impl Universe {
    /// Determines whether code owned by `site` may name class `c`: a class without access
    /// modifier everywhere; `private` in its owner and the owner's companion; `private[q]` within
    /// `q`, as a package-private Java class within its package; `protected` also in subclasses
    /// of its owner.
    pub fn is_class_accessible_from(&mut self, c: SymbolId, site: SymbolId) -> bool {
        let flags = self.symbol(c).flags;
        let modifiers = [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified];
        if !modifiers.iter().any(|&f| flags.has(f)) {
            return true;
        }
        let sites: Vec<SymbolId> = std::iter::successors(Some(site), |&s| self.symbol(s).owner).collect();
        if self.access_qualifier(c).is_some_and(|q| sites.contains(&q)) {
            return true;
        }
        let Some(owner) = self.symbol(c).owner else { return false };
        let companion = match (self.is_class(owner), self.is_module_class(owner)) {
            (false, _) => None,
            (true, true) => self.companion_class(owner),
            (true, false) => self.companion_module(owner).and_then(|m| self.module_class(m)),
        };
        if sites.contains(&owner) || companion.is_some_and(|o| sites.contains(&o)) {
            return true;
        }
        let protected = flags.has(PROTECTED) && self.is_class(owner);
        protected && sites.iter().any(|&s| self.symbol(s).kind == Kind::Class && self.derives_from(s, owner))
    }

    /// Determines whether the code being compared (`set_comparison_owner`) may name class `c`;
    /// every class where that code is not known.
    pub fn is_class_accessible_here(&mut self, c: SymbolId) -> bool {
        self.comparison_owner.is_none_or(|site| self.is_class_accessible_from(c, site))
    }
}
