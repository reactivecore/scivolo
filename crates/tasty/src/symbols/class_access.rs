// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Whether code may name a class with an access modifier (dotty's `isAccessibleFrom` of a class).

use super::{Kind, SymbolId, Universe};
use crate::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};

impl Universe {
    /// Determines whether code owned by `site` may name class `c` (dotty's `isAccessibleFrom`
    /// and `accessBoundary`): a class without access modifier, a local class and a Java
    /// `protected static` class everywhere; otherwise code within its boundary or the boundary's
    /// companion, the boundary being the owner of a `private` or `protected` class and `q` of a
    /// `private[q]` or `protected[q]` one. A Java class's package boundary does not extend to
    /// subpackages. A `protected` class is also accessible in subclasses of its owner.
    pub fn is_class_accessible_from(&mut self, c: SymbolId, site: SymbolId) -> bool {
        let flags = self.symbol(c).flags;
        let modifiers = [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified];
        if !modifiers.iter().any(|&f| flags.has(f)) && !self.access_qualifiers.contains_key(&c) {
            return true;
        }
        let Some(owner) = self.symbol(c).owner else { return true };
        let (java, protected) = (self.is_java_defined(c), flags.has(PROTECTED));
        if java && protected && self.is_module_class(owner) {
            return true;
        }
        let boundary = match self.access_qualifier(c) {
            Some(q) if !flags.has(PRIVATE) => q,
            _ => owner,
        };
        if !matches!(self.symbol(boundary).kind, Kind::Package | Kind::Class) {
            return true;
        }

        let sites: Vec<SymbolId> = std::iter::successors(Some(site), |&s| self.symbol(s).owner).collect();
        let package = sites.iter().copied().find(|&s| self.symbol(s).kind == Kind::Package);
        let within = |u: &Self, b: SymbolId| {
            sites.contains(&b) && !(java && u.symbol(b).kind == Kind::Package && package != Some(b))
        };
        let linked = match (self.is_class(boundary), self.is_module_class(boundary)) {
            (false, _) => None,
            (true, true) => self.companion_class(boundary),
            (true, false) => self.companion_module(boundary).and_then(|m| self.module_class(m)),
        };
        if within(self, boundary) || linked.is_some_and(|l| within(self, l)) {
            return true;
        }

        protected
            && self.is_class(owner)
            && sites.iter().any(|&s| self.symbol(s).kind == Kind::Class && self.derives_from(s, owner))
    }

    /// Determines whether the code being compared (`set_comparison_owner`) may name class `c`;
    /// every class where that code is not known.
    pub fn is_class_accessible_here(&mut self, c: SymbolId) -> bool {
        self.comparison_owner.is_none_or(|site| self.is_class_accessible_from(c, site))
    }
}
