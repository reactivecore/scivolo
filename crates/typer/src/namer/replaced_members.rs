// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The classpath members a unit's definitions replace.

use crate::typer::Typer;
use syntax::ast::NodeId;
use tasty::SymbolId;

impl Typer<'_> {
    /// Removes the TASTy members and the Scala library's Java classes of package `package` that a
    /// unit's member `name` replaces: those of that name and of its companion's (dotty's
    /// `isDefinedInCurrentRun`), not the JDK classes a name Scala Native gives them stands for;
    /// replacing a TASTy one of package `scala` but not `scala.scalanative` marks the run as
    /// building the Scala library. Refuses definition `tree` replacing a well-known symbol
    /// (`scala.AnyVal`, `scala.Unit`, `scala.Predef`, ...).
    pub fn unenter_classpath_members(&mut self, tree: NodeId, package: SymbolId, name: &str) {
        let base = name.strip_suffix('$').unwrap_or(name);
        for n in [base.to_owned(), format!("{base}$")] {
            for d in self.u.decls_named(package, &n) {
                let tasty = self.u.symbol(d).tree.is_some();
                if (tasty || self.u.is_library_java_stub(d)) && self.u.symbol(d).name == n {
                    if self.defn.is_well_known(d) {
                        self.unsupported(tree, "a definition replacing a well-known library symbol");
                    }
                    let full = self.u.full_name(d);
                    if tasty && full.starts_with("scala.") && !full.starts_with("scala.scalanative.") {
                        self.u.set_builds_scala_library();
                    }
                    self.u.unenter(d);
                }
            }
        }
    }
}
