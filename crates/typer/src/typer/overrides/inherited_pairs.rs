// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/OverridingPairs.scala
//! The overriding pairs of inherited members (dotty's `OverridingPairs.Cursor`) checked for
//! incompatible types.

use super::super::Typer;
use super::OverrideChecks;
use super::result_types::is_checked_term;
use std::collections::HashSet;
use tasty::SymbolId;

impl OverrideChecks {
    /// Whether an inherited member overrides another inherited member with an incompatible type,
    /// as `y` of `trait T { def y: String = "" }` over `y` of `abstract class A { def y: Int }` in
    /// `class B extends A with T`. Names the class declares are left to its own members' checks,
    /// pairs of two base classes one parent derives from to that parent's.
    pub fn incompatible_inherited(&self, t: &mut Typer<'_>) -> bool {
        let parents: Vec<SymbolId> =
            t.u.parents(self.cls).into_iter().filter_map(|p| t.u.class_symbol(p)).collect();
        let covered = |t: &mut Typer<'_>, a: SymbolId, b: SymbolId| {
            parents.iter().any(|&p| t.u.derives_from(p, a) && t.u.derives_from(p, b))
        };
        let own = term_names(t, self.cls);
        let names_of: Vec<HashSet<String>> = self.bases.iter().map(|&b| term_names(t, b)).collect();
        let mut names = Vec::new();
        for (i, &a) in self.bases.iter().enumerate() {
            for (j, &b) in self.bases.iter().enumerate().skip(i + 1) {
                if covered(t, a, b) {
                    continue;
                }
                let shared = names_of[i].intersection(&names_of[j]).filter(|&n| !own.contains(n));
                names.extend(shared.filter(|&n| !names.contains(n)).cloned().collect::<Vec<_>>());
            }
        }
        for name in names {
            let entries = self.pair_entries(t, &name);
            let mut visited = HashSet::new();
            for (k, &d) in entries.iter().enumerate() {
                if visited.contains(&d) {
                    continue;
                }
                for &m in &entries[k + 1..] {
                    let (od, om) = (t.u.symbol(d).owner, t.u.symbol(m).owner);
                    if od == om || !t.matches_loosely(self.site, d, m) {
                        continue;
                    }
                    visited.insert(m);
                    let (Some(od), Some(om)) = (od, om) else { continue };
                    if !covered(t, od, om) && self.incompatible_pair(t, d, m) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// The checked terms named `name` of the base classes, overriding candidates first: the
    /// concrete ones, then the abstract ones, each from the nearest base class on.
    fn pair_entries(&self, t: &mut Typer<'_>, name: &str) -> Vec<SymbolId> {
        let mut found = Vec::new();
        for &b in &self.bases {
            let decls = t.u.symbol(b).decls.clone();
            for d in decls.into_iter().rev() {
                let s = t.u.symbol(d);
                if s.name == name && !s.is_private() && is_checked_term(t, d) {
                    t.complete(d);
                    found.push(d);
                }
            }
        }
        let (mut concrete, deferred): (Vec<SymbolId>, Vec<SymbolId>) =
            found.into_iter().partition(|&d| t.is_concrete(d));
        concrete.extend(deferred);
        concrete
    }
}

/// The names of the non-private term members class `c` declares.
fn term_names(t: &Typer<'_>, c: SymbolId) -> HashSet<String> {
    let decls = t.u.symbol(c).decls.iter().map(|&d| t.u.symbol(d));
    decls.filter(|s| !s.is_private() && s.is_term_member()).map(|s| s.name.clone()).collect()
}
