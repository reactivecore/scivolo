//! Members of `Any` and `Object` whose selection depends on the JDK classes and interfaces.

use super::Typer;
use names::nme;
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// Whether member `sym` of `Any` or `Object`, selected from `qual`, is one a JDK interface
    /// the class of `qual` derives from declares; javalib's interfaces declare none when javalib
    /// is compiled.
    pub fn jdk_redeclares(&mut self, sym: SymbolId, qual: TypeId) -> bool {
        const REDECLARED: [(&str, &str); 22] = [
            ("java.lang.CharSequence", nme::TO_STRING),
            ("java.util.Comparator", nme::EQUALS),
            ("java.util.Collection", nme::EQUALS),
            ("java.util.Collection", nme::HASH_CODE),
            ("java.util.Map", nme::EQUALS),
            ("java.util.Map", nme::HASH_CODE),
            ("java.util.Map$.Entry", nme::EQUALS),
            ("java.util.Map$.Entry", nme::HASH_CODE),
            ("java.lang.annotation.Annotation", nme::EQUALS),
            ("java.lang.annotation.Annotation", nme::HASH_CODE),
            ("java.lang.annotation.Annotation", nme::TO_STRING),
            ("java.nio.file.Path", nme::EQUALS),
            ("java.nio.file.Path", nme::HASH_CODE),
            ("java.nio.file.Path", nme::TO_STRING),
            ("java.time.chrono.ChronoLocalDate", nme::EQUALS),
            ("java.time.chrono.ChronoLocalDate", nme::HASH_CODE),
            ("java.time.chrono.ChronoLocalDate", nme::TO_STRING),
            ("java.time.chrono.ChronoLocalDateTime", nme::EQUALS),
            ("java.time.chrono.ChronoLocalDateTime", nme::HASH_CODE),
            ("java.time.chrono.ChronoLocalDateTime", nme::TO_STRING),
            ("java.util.Set", nme::EQUALS),
            ("java.util.List", nme::EQUALS),
        ];
        let b = self.u.defn();
        if !self.u.symbol(sym).owner.is_some_and(|o| o == b.any || o == b.object) {
            return false;
        }
        let name = self.u.symbol(sym).name.clone();
        let widened = self.u.widen(qual);
        let Some(class) = self.u.class_symbol(widened) else { return false };
        REDECLARED.iter().filter(|(_, m)| *m == name).any(|(i, _)| {
            let i = self.resolve_full_name(i, true).filter(|&i| self.u.may_be_java(i));
            i.is_some_and(|i| self.u.derives_from(class, i))
        })
    }

    /// The member of `Any` the JDK's `java.lang.Class` inherits where nativelib's `_Class`
    /// overrides `sym` (`equals`, `hashCode`); `sym` otherwise.
    pub fn jdk_inherited(&mut self, sym: SymbolId) -> SymbolId {
        let s = self.u.symbol(sym);
        let name = s.name.clone();
        let Some(owner) = s.owner.filter(|_| matches!(name.as_str(), nme::EQUALS | nme::HASH_CODE)) else {
            return sym;
        };
        if self.u.full_name(owner) != "java.lang.Class" {
            return sym;
        }
        let any = self.u.defn().any;
        self.u.decls_named(any, &name).first().copied().unwrap_or(sym)
    }
}
