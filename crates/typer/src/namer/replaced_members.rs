// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The classpath members a unit's definitions replace.

use crate::tpd::TNodeId;
use crate::typer::Typer;
use tasty::SymbolId;

impl Typer<'_> {
    /// Removes the TASTy members and the Scala library's Java classes of package `package` that a
    /// unit's member `name` replaces: those of that name and of its companion's (dotty's
    /// `isDefinedInCurrentRun`), not the JDK classes a name Scala Native gives them stands for;
    /// replacing a TASTy one of package `scala` but not `scala.scalanative` marks the run as
    /// building the Scala library. Returns whether a removed one is a well-known symbol.
    pub fn unenter_classpath_members(&mut self, package: SymbolId, name: &str) -> bool {
        let base = name.strip_suffix('$').unwrap_or(name);
        let mut well_known = false;
        for n in [base.to_owned(), format!("{base}$")] {
            for d in self.u.decls_named(package, &n) {
                let tasty = self.u.symbol(d).tree.is_some();
                if (tasty || self.u.is_library_java_stub(d)) && self.u.symbol(d).name == n {
                    well_known |= self.defn.is_well_known(d);
                    let full = self.u.full_name(d);
                    if tasty && full.starts_with("scala.") && !full.starts_with("scala.scalanative.") {
                        self.u.set_builds_scala_library();
                    }
                    self.u.unenter(d);
                }
            }
        }
        well_known
    }

    /// Refuses each definition replacing a well-known symbol (`scala.AnyVal`, `scala.Unit`,
    /// `scala.Predef`, ...) whose symbol a typed tree outside it and its companion refers to or
    /// has as its type's class, as `V extends AnyVal` or `ClassTag[Unit]` do.
    pub fn check_well_known_uses(&mut self) {
        let replacing = std::mem::take(&mut self.replacing_well_known);
        if replacing.is_empty() {
            return;
        }
        let mut spans = Vec::new();
        for &(_, sym) in &replacing {
            let defined = self.with_companion(sym).into_iter().filter_map(|s| self.syms.def_trees.get(&s));
            spans.extend(defined.map(|&t| self.ast.span(t)));
        }
        let mut used = vec![false; replacing.len()];
        for node in (0..self.tpd.len()).map(TNodeId::new) {
            let span = self.tpd.span(node);
            if span.exists() && spans.iter().any(|s| s.contains(span)) {
                continue;
            }
            let class = self.tpd.get(node).tpe.and_then(|t| self.u.class_symbol(t));
            for (i, &(_, sym)) in replacing.iter().enumerate() {
                used[i] |= self.tpd.sym(node) == Some(sym) || class == Some(sym);
            }
        }
        for (&(tree, _), _) in replacing.iter().zip(used).filter(|(_, used)| *used) {
            self.unsupported(tree, "a definition replacing a well-known library symbol");
        }
    }

    /// The members of `sym`'s owner named as `sym` or its companion.
    fn with_companion(&mut self, sym: SymbolId) -> Vec<SymbolId> {
        let Some(owner) = self.u.symbol(sym).owner else { return vec![sym] };
        let name = self.u.symbol(sym).name.clone();
        let base = name.strip_suffix('$').unwrap_or(&name);
        let mut syms = self.u.decls_named(owner, base);
        syms.extend(self.u.decls_named(owner, &format!("{base}$")));
        syms
    }
}
