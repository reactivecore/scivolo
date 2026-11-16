// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! The classpath members a unit's definitions replace.

use crate::tpd::TNodeId;
use crate::typer::Typer;
use tasty::SymbolId;

impl Typer<'_> {
    /// Removes the TASTy members and the Scala library's Java classes of package `package` that a
    /// unit's member `name` replaces: those of that name and of its companion's (dotty's
    /// `isDefinedInCurrentRun`), not the JDK classes a name Scala Native gives them stands for;
    /// replacing a TASTy one of package `scala` but not `scala.scalanative` marks the run as
    /// building the Scala library. Keeps the removed well-known symbols for
    /// `replace_well_known`.
    pub fn unenter_classpath_members(&mut self, package: SymbolId, name: &str) {
        let base = name.strip_suffix('$').unwrap_or(name);
        for n in [base.to_owned(), format!("{base}$")] {
            for d in self.u.decls_named(package, &n) {
                let tasty = self.u.symbol(d).tree.is_some();
                if (tasty || self.u.is_library_java_stub(d)) && self.u.symbol(d).name == n {
                    if self.defn.is_well_known(d) {
                        self.replaced_well_known.push(d);
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

    /// Makes unit definition `sym` the well-known symbol it replaces, if any: in `Defn`, the
    /// universe's and the root imports, as dotty's `Definitions` resolve to the definitions of
    /// the run that compiles the library.
    pub fn replace_well_known(&mut self, sym: SymbolId) {
        let (name, kind) = (&self.u.symbol(sym).name, self.u.symbol(sym).kind);
        let same = |u: &tasty::Universe, d: SymbolId| u.symbol(d).name == *name && u.symbol(d).kind == kind;
        let Some(i) = self.replaced_well_known.iter().position(|&d| same(self.u, d)) else { return };
        let old = self.replaced_well_known.swap_remove(i);
        self.defn.replace(old, sym);
        self.u.replace_defn(old, sym);
        self.replaced_by.push((old, sym));
        self.replace_root_imports(old, sym);
    }

    /// Refuses each typed tree that still refers to a well-known symbol a definition of the unit
    /// replaced, or has it as its type's class.
    pub fn check_well_known_uses(&mut self) {
        let replaced = std::mem::take(&mut self.replaced_by);
        if replaced.is_empty() {
            return;
        }
        for node in (0..self.tpd.len()).map(TNodeId::new) {
            let class = self.tpd.get(node).tpe.and_then(|t| self.u.class_symbol(t));
            let sym = self.tpd.sym(node);
            if replaced.iter().any(|&(old, _)| sym == Some(old) || class == Some(old)) {
                let span = self.tpd.span(node);
                self.unsupported_at(
                    span,
                    "a reference to a library symbol a definition of the unit replaces",
                );
            }
        }
    }
}
