// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenUtil.scala
//! The positions of the implicit qualifiers of identifiers (nscplugin's `cachedDesugarIdent`).

use super::Gen;
use syntax::ast::Span;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// Determines the span of the implicit qualifier of identifier `t` read as an expression:
    /// nscplugin desugars the identifiers of each type once per source file and positions the
    /// shared qualifier at the start of the first one with a span, so `this.x` of every later
    /// `x` takes that span. An identifier from another source keeps its own span.
    pub fn ident_qualifier_span(&mut self, t: TNodeId) -> Span {
        let span = self.l.tpd.span(t);
        if !span.exists() || span.is_in_other_source() {
            return span;
        }
        let key = (self.l.sources.index(span.start()), self.l.tpd.tpe(t));
        *self.ident_qualifiers.entry(key).or_insert(span.start_pos())
    }
}
