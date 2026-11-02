// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! dotty's `checkAllOverrides` step 4: a member with an `override` modifier overrides some member.

use super::Typer;
use syntax::ast::flags as f;
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// The first member of class `cls` with an `override` modifier that overrides nothing, seen
    /// from `site` with base classes `bases`. A repeated parameter matches only a repeated one.
    pub fn overriding_nothing(&mut self, cls: SymbolId, site: TypeId, bases: &[SymbolId]) -> Option<SymbolId> {
        let mut bases = bases.to_vec();
        bases.extend(self.self_type_bases(cls));
        let own: Vec<SymbolId> = self.u.symbol(cls).decls.clone();
        own.into_iter().find(|&d| {
            let flags = self.syms.flags.get(&d).copied().unwrap_or_default();
            flags.is_one_of(f::OVERRIDE.or(f::ABS_OVERRIDE))
                && !flags.is(f::SYNTHETIC)
                && !bases.iter().any(|&b| b != cls && self.has_matching_sym(site, b, d))
        })
    }

    fn has_matching_sym(&mut self, site: TypeId, b: SymbolId, d: SymbolId) -> bool {
        let name = self.u.symbol(d).name.clone();
        if !self.u.symbol(d).kind.is_term() {
            let decls = &self.u.symbol(b).decls;
            return decls.iter().any(|&m| {
                let s = self.u.symbol(m);
                s.name == name && !s.kind.is_term() && !s.flags.has(tasty::tags::PRIVATE)
            });
        }
        self.overridable(b, &name).into_iter().any(|m| {
            self.complete(m);
            self.same_member(site, d, m)
        })
    }
}
