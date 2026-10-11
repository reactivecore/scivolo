//! `@static` members (dotty's `CheckStatic`), kept members of their objects.

use super::annotations::STATIC;
use crate::typer::Typer;
use syntax::ast::NodeId;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Refuses `@static` member `sym`, annotated by `annot`, where `CheckStatic` rejects it or
    /// where another member of its object has its name.
    pub fn check_static(&mut self, sym: SymbolId, annot: NodeId) {
        let owner = self.u.symbol(sym).owner.expect("a member");
        let companion = if self.u.is_module_class(owner) { self.u.companion_class(owner) } else { None };
        let Some(companion) = companion else {
            return self
                .unsupported_at(self.ast.span(annot), "@static outside an object with a companion class");
        };
        let name = self.u.symbol(sym).name.clone();
        let flags = self.syms.flags.get(&sym).copied().unwrap_or_default();
        let decls = self.u.symbol(owner).decls.clone();
        let is_field = |t: &Self, d: SymbolId| t.u.symbol(d).kind == Kind::Val;
        let field_after = is_field(self, sym)
            && decls.iter().take_while(|&&d| d != sym).any(|&d| is_field(self, d) && !self.is_static(d));
        let same_name = |t: &Self, d: SymbolId| {
            let s = t.u.symbol(d);
            d != sym && s.kind.is_term() && (s.name == name || s.target.as_ref() == Some(&name))
        };
        let clash = decls.iter().any(|&d| same_name(self, d));
        let in_companion = self.u.decl(companion, &name).unwrap_or_default().into_iter().any(|d| d != sym);
        let bases: Vec<SymbolId> = self.u.linearization(owner).into_iter().skip(1).collect();
        let overrides = bases.into_iter().any(|b| self.u.symbol(b).decls.iter().any(|&d| same_name(self, d)));
        let trait_var = flags.is(f::MUTABLE) && self.u.symbol(companion).flags.has(tasty::tags::TRAIT);
        if flags.is(f::LAZY) || clash || in_companion || overrides || trait_var || field_after {
            self.unsupported_at(self.ast.span(annot), "@static member clashing or misplaced");
        }
    }

    fn is_static(&self, s: SymbolId) -> bool {
        let annots = self.syms.annotations.get(&s).into_iter().flatten();
        annots.into_iter().any(|&(c, _)| self.u.full_name(c) == STATIC)
    }
}
