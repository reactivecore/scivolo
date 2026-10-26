// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Which member of a base class another member hides in a class's member lookup.

use super::Typer;
use tasty::SymbolId;

impl Typer<'_> {
    /// Whether concrete `m` of a base class replaces abstract `o` found before it in class `c`.
    pub fn concrete_replaces(&mut self, c: SymbolId, o: SymbolId, m: SymbolId) -> bool {
        self.is_concrete(m) && !self.is_concrete(o) && !self.declared_below(c, o, m)
    }

    /// Whether one of `o` and `m` is a Java field or a private value and the other a method with
    /// an empty parameter list.
    pub fn signatures_differ(&mut self, o: SymbolId, m: SymbolId) -> bool {
        let value = |t: &mut Self, s: SymbolId| {
            t.u.symbol(s).kind == tasty::Kind::Val
                && (t.u.symbol(s).flags.has(tasty::tags::PRIVATE) || t.is_java_member(s))
        };
        value(self, o) && self.empty_param_list(m) || value(self, m) && self.empty_param_list(o)
    }

    /// Whether `s` is a method with an empty parameter list, whose signature a value's does not
    /// match.
    fn empty_param_list(&mut self, s: SymbolId) -> bool {
        let info = self.u.info(s);
        self.u.symbol(s).kind == tasty::Kind::Def
            && matches!(self.u.types().get(info), tasty::Type::Method { params, .. } if params.is_empty())
    }

    /// Whether `o`, a member of class `c` and not private, hides `m` of a base class of its class:
    /// `c` declares `o`, or each parent of `c` that inherits `m` does so through `o`'s class (dotty's
    /// `mapInherited` dropping what a class's own members match).
    fn declared_below(&mut self, c: SymbolId, o: SymbolId, m: SymbolId) -> bool {
        let (Some(a), Some(b)) = (self.u.symbol(o).owner, self.u.symbol(m).owner) else { return false };
        if a == b || !self.u.derives_from(a, b) || self.u.symbol(o).flags.has(tasty::tags::PRIVATE) {
            return false;
        }
        let parents: Vec<SymbolId> =
            self.u.parents(c).into_iter().filter_map(|p| self.u.class_symbol(p)).collect();
        a == c || parents.into_iter().all(|p| !self.u.derives_from(p, b) || self.u.derives_from(p, a))
    }
}
