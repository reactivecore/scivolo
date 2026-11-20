//! The linking guard of a class extending Scala 2 classes of jars (ADR 0128).

use super::descriptors::method_descriptor;
use super::{Gen, method_name, unsupported};
use lower::Result;
use syntax::ast::Span;
use tasty::tags::PRIVATE;
use tasty::{Kind, SymbolId};

impl Gen<'_, '_> {
    /// Checks that each method and getter of the Scala 2 base classes of class `c` of a jar
    /// erases as its class file declares it, so that overriding and bridges in `c` follow the
    /// erasure Scala 2 gave it; refuses `c` at `span` otherwise.
    pub fn check_scala2_bases(&mut self, c: SymbolId, span: Span) -> Result<()> {
        for base in self.l.u.linearization(c) {
            if base == c || !self.l.u.is_pickled(base) {
                continue;
            }
            let file = self.class_name(base);
            for m in self.l.u.symbol(base).decls.clone() {
                let s = self.l.u.symbol(m);
                if !matches!(s.kind, Kind::Def | Kind::Val)
                    || s.flags.has(PRIVATE)
                    || self.l.u.is_constructor(m)
                {
                    continue;
                }
                let (name, desc) = (method_name(self.l.u, m), method_descriptor(self.l.u, m));
                if let Err(why) = self.l.u.check_scala2_link(m, &file, &name, &desc) {
                    return unsupported(span, why);
                }
            }
        }
        Ok(())
    }
}
