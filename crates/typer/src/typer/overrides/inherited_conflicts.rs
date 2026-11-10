// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! Concrete members of traits inherited in conflict, which dotty's `RefChecks.checkAllOverrides`
//! rejects.

use super::super::Typer;
use super::{OverrideChecks, overridable};
use syntax::ast::flags as f;
use tasty::tags::{CASE, OVERRIDE, TRAIT};
use tasty::{SymbolId, Universe};

/// The members the synthesized members of a case class override where `Any`, `Object`,
/// `Product` or `Equals` declares them.
const SYNTHESIZED: [&str; 8] = [
    "hashCode",
    "equals",
    "toString",
    "canEqual",
    "productArity",
    "productPrefix",
    "productElement",
    "productElementName",
];

impl OverrideChecks {
    /// Whether the class inherits two concrete members of its base classes and neither overrides
    /// the other, as `f` of `trait A { def f = 1 }; trait B { def f = 2 }; class C extends A, B`.
    pub fn inherited_conflict(&self, t: &mut Typer<'_>) -> bool {
        let own_names: Vec<String> =
            t.u.symbol(self.cls).decls.iter().map(|&d| t.u.symbol(d).name.clone()).collect();
        let traits: Vec<SymbolId> =
            self.bases.iter().copied().filter(|&b| t.u.symbol(b).flags.has(TRAIT)).collect();
        for &tr in &traits {
            let names: Vec<String> =
                t.u.symbol(tr).decls.iter().map(|&d| t.u.symbol(d).name.clone()).collect();
            for name in names.iter().filter(|n| !own_names.contains(n) && n.as_str() != "<init>") {
                let concrete: Vec<SymbolId> = self
                    .bases
                    .iter()
                    .flat_map(|&b| overridable(t.u, b, name))
                    .filter(|&m| t.is_concrete(m))
                    .collect();
                let Some((&winner, rest)) = concrete.split_first() else { continue };
                let Some(winner_owner) = t.u.symbol(winner).owner else { continue };
                if is_synthesized_case_member(t.u, self.cls, winner_owner, name) {
                    continue;
                }
                for &m in rest {
                    let owner = t.u.symbol(m).owner.expect("a member");
                    if t.u.derives_from(winner_owner, owner) || !t.same_member(self.site, winner, m) {
                        continue;
                    }
                    if !self.overrides_through(t, winner, (winner_owner, owner))
                        && !self.overrides_through(t, m, (owner, winner_owner))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Whether member `m` of class `a`, declared `override`, overrides the member of class `b`.
    fn overrides_through(&self, t: &mut Typer<'_>, m: SymbolId, (a, b): (SymbolId, SymbolId)) -> bool {
        let overrides = match t.syms.flags.get(&m) {
            Some(fl) => fl.is(f::OVERRIDE) || fl.is(f::ABS_OVERRIDE),
            None => t.u.symbol(m).flags.has(OVERRIDE),
        };
        overrides && self.overrides_in_common(t, (a, b), m)
    }

    /// Whether classes `a` and `b` both derive from a third class with a member `m` matches, or
    /// `b` is a base class of `a`'s self type (dotty's `extendedOverriddenSymbols`).
    fn overrides_in_common(&self, t: &mut Typer<'_>, (a, b): (SymbolId, SymbolId), m: SymbolId) -> bool {
        let name = t.u.symbol(m).name.clone();
        if t.u.self_type_bases(a).contains(&b) {
            return true;
        }
        let bases: Vec<SymbolId> = t.u.linearization(a).into_iter().skip(1).collect();
        let common: Vec<SymbolId> = bases.into_iter().filter(|&c| c != b && t.u.derives_from(b, c)).collect();
        common.into_iter().any(|c| {
            let members = overridable(t.u, c, &name);
            members.into_iter().any(|o| t.same_member(self.site, m, o))
        })
    }
}

/// Whether case class `cls` gets a synthetic member `name` overriding the inherited ones, the
/// first of which `owner` declares (dotty's `SyntheticMembers.existingDef`).
fn is_synthesized_case_member(u: &Universe, cls: SymbolId, owner: SymbolId, name: &str) -> bool {
    let original = ["scala.Any", "java.lang.Object", "scala.Product", "scala.Equals"];
    u.symbol(cls).flags.has(CASE)
        && SYNTHESIZED.contains(&name)
        && original.contains(&u.full_name(owner).as_str())
}
