// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The package or named class enclosing an owner.

use super::Typer;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Determines the package enclosing `owner`, or `owner` itself if it is one (dotty's
    /// `enclosingPackageClass`).
    pub fn enclosing_package(&self, owner: SymbolId) -> SymbolId {
        let mut o = owner;
        while self.u.symbol(o).kind != Kind::Package {
            match self.u.symbol(o).owner {
                Some(p) => o = p,
                None => break,
            }
        }
        o
    }

    /// Determines the class or package enclosing `owner`, or `owner` itself, whose name or
    /// object's name is `name` (dotty's `enclosingClassNamed`).
    pub fn enclosing_class_named(&self, owner: SymbolId, name: &str) -> Option<SymbolId> {
        let mut o = Some(owner);
        while let Some(s) = o {
            let sym = self.u.symbol(s);
            let named =
                sym.name == name || self.u.is_module_class(s) && tasty::module_name(&sym.name) == Some(name);
            if named && matches!(sym.kind, Kind::Class | Kind::Package) {
                return Some(s);
            }
            o = sym.owner;
        }
        None
    }
}
