// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Which member of a base class another member hides in a class's member lookup.

use super::Typer;
use tasty::tags::PRIVATE;
use tasty::{Kind, SymbolId, Universe};

impl Typer<'_> {
    /// Whether concrete `m` of a base class replaces abstract `o` found before it in class `c`.
    pub fn concrete_replaces(&mut self, c: SymbolId, o: SymbolId, m: SymbolId) -> bool {
        self.is_concrete(m) && !self.is_concrete(o) && !declared_below(self.u, c, o, m)
    }

    /// Whether one of `o` and `m` is a Java field or a private value and the other a method with
    /// an empty parameter list.
    pub fn signatures_differ(&mut self, o: SymbolId, m: SymbolId) -> bool {
        let value = |t: &mut Self, s: SymbolId| {
            t.u.symbol(s).kind == Kind::Val && (t.u.symbol(s).flags.has(PRIVATE) || t.is_java_member(s))
        };
        value(self, o) && empty_param_list(self.u, m) || value(self, m) && empty_param_list(self.u, o)
    }
}

/// Whether `s` is a method with an empty parameter list, whose signature a value's does not
/// match.
fn empty_param_list(u: &mut Universe, s: SymbolId) -> bool {
    let info = u.info(s);
    u.symbol(s).kind == Kind::Def && u.method(info).is_some_and(|m| m.params.is_empty())
}

/// Whether `o`, a member of class `c` and not private, hides `m` of a base class of its class:
/// `c` declares `o`, or each parent of `c` that inherits `m` does so through `o`'s class (dotty's
/// `mapInherited` dropping what a class's own members match).
fn declared_below(u: &mut Universe, c: SymbolId, o: SymbolId, m: SymbolId) -> bool {
    let (Some(a), Some(b)) = (u.symbol(o).owner, u.symbol(m).owner) else { return false };
    if a == b || !u.derives_from(a, b) || u.symbol(o).flags.has(PRIVATE) {
        return false;
    }
    let parents: Vec<SymbolId> = u.parents(c).into_iter().filter_map(|p| u.class_symbol(p)).collect();
    a == c || parents.into_iter().all(|p| !u.derives_from(p, b) || u.derives_from(p, a))
}
