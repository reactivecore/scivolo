//! `@static` members (dotty's `CheckStatic`), kept members of their objects.

use super::annotations::STATIC;
use crate::typer::Typer;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Refuses `@static` member `sym`, annotated by `annot`, where `CheckStatic` rejects it, and
    /// a `@static` value initialized by more than a literal, which the object's constructor
    /// initializes here.
    pub fn check_static(&mut self, sym: SymbolId, annot: NodeId) {
        let span = self.ast.span(annot);
        let owner = self.u.symbol(sym).owner.expect("a member");
        let companion = if self.u.is_module_class(owner) { self.u.companion_class(owner) } else { None };
        let Some(companion) = companion else {
            return self.unsupported_at(span, "@static outside an object with a companion class");
        };
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
            return self.unsupported_at(span, "@static member CheckStatic rejects");
        }
        let tree = self.syms.by_tree.iter().find(|&(_, &s)| s == sym).map(|(&t, _)| t);
        let rhs = tree.and_then(|t| match *self.ast.tree(t) {
            Tree::ValDef { rhs, .. } => rhs,
            _ => None,
        });
        if rhs.is_some_and(|r| !matches!(self.ast.tree(r), Tree::Literal { .. })) {
            self.unsupported_at(span, "@static value initialized by more than a literal");
        }
    }

    fn is_static(&self, s: SymbolId) -> bool {
        self.u.annotations(s).iter().any(|&c| self.u.full_name(c) == STATIC)
    }
}
