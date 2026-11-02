// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! dotty's `checkAllOverrides` step 4, a member with an `override` modifier overrides some member,
//! where no base member has its name or each one differs in a repeated parameter.

use super::Typer;
use syntax::ast::flags as f;
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// Whether a member of class `cls` with an `override` modifier overrides nothing, seen from
    /// `site` with base classes `bases`.
    pub fn overrides_nothing(&mut self, cls: SymbolId, site: TypeId, bases: &[SymbolId]) -> bool {
        let mut bases = bases.to_vec();
        bases.extend(self.self_type_bases(cls));
        bases.retain(|&b| b != cls);
        let own: Vec<SymbolId> = self.u.symbol(cls).decls.clone();
        own.into_iter().any(|d| {
            let flags = self.syms.flags.get(&d).copied().unwrap_or_default();
            flags.is_one_of(f::OVERRIDE.or(f::ABS_OVERRIDE))
                && !flags.is(f::SYNTHETIC)
                && self.matches_nothing(site, &bases, d)
        })
    }

    fn matches_nothing(&mut self, site: TypeId, bases: &[SymbolId], d: SymbolId) -> bool {
        let name = self.u.symbol(d).name.clone();
        if !self.u.symbol(d).kind.is_term() {
            return !bases.iter().any(|&b| {
                self.u.symbol(b).decls.iter().any(|&m| {
                    let s = self.u.symbol(m);
                    s.name == name && !s.kind.is_term() && !s.flags.has(tasty::tags::PRIVATE)
                })
            });
        }
        let named: Vec<SymbolId> = bases.iter().flat_map(|&b| self.overridable(b, &name)).collect();
        named.into_iter().all(|m| {
            self.complete(m);
            self.incompatible_repeated_param(d, m) && !self.same_member(site, d, m)
        })
    }

    /// Whether in some parameter clause the last parameters of `a` and `b` differ in being repeated.
    fn incompatible_repeated_param(&mut self, a: SymbolId, b: SymbolId) -> bool {
        let (ia, ib) = (self.u.info(a), self.u.info(b));
        let (ca, cb) = (self.u.param_clauses(ia), self.u.param_clauses(ib));
        ca.into_iter().zip(cb).any(|((_, x), (_, y))| match (x.last(), y.last()) {
            (Some(&x), Some(&y)) => {
                self.repeated_param_elem(x).is_some() != self.repeated_param_elem(y).is_some()
            }
            _ => false,
        })
    }
}
