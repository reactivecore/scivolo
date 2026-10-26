// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! Concrete members of traits inherited in conflict, which dotty's `RefChecks.checkAllOverrides`
//! rejects, made to fall back.

use super::Typer;
use syntax::ast::Span;
use syntax::ast::flags as f;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Refuses two concrete members of base classes `bases` of class `cls` that `cls` inherits
    /// and neither overrides the other.
    pub fn check_inherited_conflicts(&mut self, cls: SymbolId, site: TypeId, bases: &[SymbolId], span: Span) {
        let own_names: Vec<String> =
            self.u.symbol(cls).decls.iter().map(|&d| self.u.symbol(d).name.clone()).collect();
        let traits: Vec<SymbolId> =
            bases.iter().copied().filter(|&b| self.u.symbol(b).flags.has(tasty::tags::TRAIT)).collect();
        for &t in &traits {
            let names: Vec<String> =
                self.u.symbol(t).decls.iter().map(|&d| self.u.symbol(d).name.clone()).collect();
            for name in names.iter().filter(|n| !own_names.contains(n) && n.as_str() != "<init>") {
                let concrete: Vec<SymbolId> = bases
                    .iter()
                    .flat_map(|&b| self.overridable(b, name))
                    .filter(|&m| self.is_concrete(m))
                    .collect();
                let Some((&winner, rest)) = concrete.split_first() else { continue };
                let Some(winner_owner) = self.u.symbol(winner).owner else { continue };
                for &m in rest {
                    let owner = self.u.symbol(m).owner.expect("a member");
                    if self.u.derives_from(winner_owner, owner) || !self.same_member(site, winner, m) {
                        continue;
                    }
                    if !self.overrides_through(site, winner, (winner_owner, owner))
                        && !self.overrides_through(site, m, (owner, winner_owner))
                    {
                        return self.unsupported_at(span, "conflicting inherited members");
                    }
                }
            }
        }
    }

    /// Whether member `m` of class `a`, declared `override`, overrides the member of class `b`.
    fn overrides_through(&mut self, site: TypeId, m: SymbolId, (a, b): (SymbolId, SymbolId)) -> bool {
        let overrides = match self.syms.flags.get(&m) {
            Some(fl) => fl.is(f::OVERRIDE) || fl.is(f::ABS_OVERRIDE),
            None => self.u.symbol(m).flags.has(tasty::tags::OVERRIDE),
        };
        overrides && self.overrides_in_common(site, (a, b), m)
    }

    /// Whether classes `a` and `b` both derive from a third class with a member `m` matches, or
    /// `b` is a base class of `a`'s self type (dotty's `extendedOverriddenSymbols`).
    fn overrides_in_common(&mut self, site: TypeId, (a, b): (SymbolId, SymbolId), m: SymbolId) -> bool {
        let name = self.u.symbol(m).name.clone();
        if self.self_type_bases(a).contains(&b) {
            return true;
        }
        let bases: Vec<SymbolId> = self.u.linearization(a).into_iter().skip(1).collect();
        let common: Vec<SymbolId> =
            bases.into_iter().filter(|&c| c != b && self.u.derives_from(b, c)).collect();
        common.into_iter().any(|c| {
            let members = self.overridable(c, &name);
            members.into_iter().any(|o| self.same_member(site, m, o))
        })
    }

    /// The base classes of the classes in the given self type of class `c`.
    fn self_type_bases(&mut self, c: SymbolId) -> Vec<SymbolId> {
        let Some(self_type) = self.u.given_self_type(c) else { return Vec::new() };
        let mut todo = vec![self_type];
        let mut out = Vec::new();
        while let Some(t) = todo.pop() {
            if let Type::And(x, y) = *self.u.types().get(t) {
                todo.extend([x, y]);
            } else if let Some(class) = self.u.class_symbol(t) {
                out.extend(self.u.linearization(class));
            }
        }
        out
    }
}
