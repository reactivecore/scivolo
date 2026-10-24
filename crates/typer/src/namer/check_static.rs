// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/CheckStatic.scala
//! `@static` members (dotty's `CheckStatic`), kept members of their objects.

use super::annotations::STATIC;
use crate::typer::Typer;
use syntax::ast::NodeId;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Refuses `@static` member `sym`, annotated by `annot`, where `CheckStatic` rejects it.
    pub fn check_static(&mut self, sym: SymbolId, annot: NodeId) {
        let span = self.ast.span(annot);
        let owner = self.u.symbol(sym).owner.expect("a member");
        let companion = if self.u.is_module_class(owner) { self.u.companion_class(owner) } else { None };
        let Some(companion) = companion else {
            return self.unsupported_at(span, "@static outside an object with a companion class");
        };
        if !self.is_static_owner(owner) {
            return self.unsupported_at(span, "@static in an object that is not static");
        }
        let name = self.u.symbol(sym).name.clone();
        let flags = self.syms.flags.get(&sym).copied().unwrap_or_default();
        let is_field = |t: &Self, d: SymbolId| t.u.symbol(d).kind == Kind::Val;
        let decls = self.u.symbol(owner).decls.clone();
        let field_after = is_field(self, sym)
            && decls.iter().take_while(|&&d| d != sym).any(|&d| is_field(self, d) && !self.is_static(d));
        let in_companion = !self.class_members(companion, &name).is_empty();
        let site = crate::defn::type_ref(self.u, owner);
        let overridden: Vec<SymbolId> = self
            .class_members(owner, &name)
            .into_iter()
            .filter(|&m| self.u.symbol(m).owner != Some(owner))
            .collect();
        let overrides = overridden.into_iter().any(|m| self.same_member(site, sym, m));
        let trait_var = flags.is(f::MUTABLE) && self.u.symbol(companion).flags.has(tasty::tags::TRAIT);
        if flags.is(f::LAZY) || in_companion || overrides || trait_var || field_after {
            self.unsupported_at(span, "@static member CheckStatic rejects");
        }
    }

    fn is_static(&self, s: SymbolId) -> bool {
        self.u.annotations(s).iter().any(|&c| self.u.full_name(c) == STATIC)
    }
}
