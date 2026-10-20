//! Members of `Any` and `Object` whose selection depends on the JDK classes and interfaces.

use super::Typer;
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// Whether member `sym` of `Any` or `Object`, selected from `qual`, is one a JDK interface
    /// the class of `qual` derives from declares.
    pub fn jdk_redeclares(&mut self, sym: SymbolId, qual: TypeId) -> bool {
        const REDECLARED: [(&str, &str); 22] = [
            ("java.lang.CharSequence", "toString"),
            ("java.util.Comparator", "equals"),
            ("java.util.Collection", "equals"),
            ("java.util.Collection", "hashCode"),
            ("java.util.Map", "equals"),
            ("java.util.Map", "hashCode"),
            ("java.util.Map$.Entry", "equals"),
            ("java.util.Map$.Entry", "hashCode"),
            ("java.lang.annotation.Annotation", "equals"),
            ("java.lang.annotation.Annotation", "hashCode"),
            ("java.lang.annotation.Annotation", "toString"),
            ("java.nio.file.Path", "equals"),
            ("java.nio.file.Path", "hashCode"),
            ("java.nio.file.Path", "toString"),
            ("java.time.chrono.ChronoLocalDate", "equals"),
            ("java.time.chrono.ChronoLocalDate", "hashCode"),
            ("java.time.chrono.ChronoLocalDate", "toString"),
            ("java.time.chrono.ChronoLocalDateTime", "equals"),
            ("java.time.chrono.ChronoLocalDateTime", "hashCode"),
            ("java.time.chrono.ChronoLocalDateTime", "toString"),
            ("java.util.Set", "equals"),
            ("java.util.List", "equals"),
        ];
        let b = self.u.builtins();
        if !self.u.symbol(sym).owner.is_some_and(|o| o == b.any || o == b.object) {
            return false;
        }
        let name = self.u.symbol(sym).name.clone();
        let widened = self.u.widen(qual);
        let Some(class) = self.u.class_symbol(widened) else { return false };
        REDECLARED
            .iter()
            .filter(|(_, m)| *m == name)
            .any(|(i, _)| self.resolve_full_name(i, true).is_some_and(|i| self.u.derives_from(class, i)))
    }

    /// The member of `Any` the JDK's `java.lang.Class` inherits where nativelib's `_Class`
    /// overrides `sym` (`equals`, `hashCode`); `sym` otherwise.
    pub fn jdk_inherited(&mut self, sym: SymbolId) -> SymbolId {
        let s = self.u.symbol(sym);
        let name = s.name.clone();
        let Some(owner) = s.owner.filter(|_| matches!(name.as_str(), "equals" | "hashCode")) else {
            return sym;
        };
        if self.u.full_name(owner) != "java.lang.Class" {
            return sym;
        }
        let any = self.u.builtins().any;
        self.u.decl(any, &name).unwrap_or_default().first().copied().unwrap_or(sym)
    }
}
