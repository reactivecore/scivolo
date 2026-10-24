//! Checks of the trees macros return: `this` only of the classes enclosing it, each symbol
//! defined once, an existing one only in place of the definition it replaces.

use super::macro_definitions::is_definition;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::{Kind, SymbolId};

impl Inlining<'_> {
    /// Refuses tree `t`, owned by `owner`, if it defines a symbol twice or refers to the `this`
    /// of a class other than a static object that does not enclose the reference.
    pub fn check_macro_tree(&self, t: TNodeId, owner: SymbolId) -> Result<(), Unsupported> {
        let (mut todo, mut defined) = (vec![(t, owner)], FxHashSet::default());
        while let Some((n, owner)) = todo.pop() {
            let tree = self.tpd.tree(n);
            if let (T::This { .. }, Some(c)) = (tree, self.tpd.sym(n))
                && !(self.u.is_module_class(c) && self.u.is_static_symbol(c))
                && !std::iter::successors(Some(owner), |&o| self.u.symbol(o).owner).any(|o| o == c)
            {
                return Err((self.tpd.span(n), "a macro's this of a class not enclosing it"));
            }
            if (is_definition(tree) || matches!(tree, T::Bind { .. }))
                && let Some(s) = self.tpd.sym(n)
                && !defined.insert(s)
            {
                return Err((self.tpd.span(n), "a macro tree defining a symbol twice"));
            }
            let defines = is_definition(tree) || matches!(tree, T::Template { .. });
            let inner = self.tpd.sym(n).filter(|_| defines).unwrap_or(owner);
            let call = if let T::Inlined { call, .. } = *tree { call } else { None };
            todo.extend(self.tpd.children(n).into_iter().filter(|&c| Some(c) != call).map(|c| (c, inner)));
        }
        Ok(())
    }

    /// Refuses definitions `made` that a macro annotation on `def` with companion `companion`
    /// returned if they add one beside a top-level class, define a symbol twice, or define one
    /// neither of the definitions they replace nor made from `fresh` on (dotty's `checkMacroDef`).
    pub fn check_made(
        &self,
        made: &[TNodeId],
        (def, companion): (TNodeId, Option<TNodeId>),
        fresh: SymbolId,
        span: Span,
    ) -> Result<(), Unsupported> {
        let is_type = |d: TNodeId| matches!(*self.tpd.tree(d), T::TypeDef { rhs, .. } if !matches!(self.tpd.tree(rhs), T::Template { .. }));
        if made.iter().any(|&d| is_type(d)) {
            return Err((span, "a macro annotation returning a type"));
        }
        let (sym, companion_sym) = (self.tpd.sym(def), companion.and_then(|c| self.tpd.sym(c)));
        let owner = sym.and_then(|s| self.u.symbol(s).owner);
        let top_level = sym.is_some_and(|s| self.u.is_class(s))
            && owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Package);
        if top_level && made.iter().any(|&d| self.tpd.sym(d) != sym && self.tpd.sym(d) != companion_sym) {
            return Err((span, "a macro annotation adding a top-level definition"));
        }
        let returned = companion.filter(|_| made.iter().any(|&d| self.tpd.sym(d) == companion_sym));
        let replaced = std::iter::once(def).chain(returned);
        let known: FxHashSet<SymbolId> = replaced.flat_map(|d| self.definition_symbols(d)).collect();
        let mut seen = FxHashSet::default();
        for s in made.iter().flat_map(|&d| self.definition_symbols(d)) {
            if !known.contains(&s) && s.0 < fresh.0 {
                return Err((span, "a macro annotation defining a symbol defined elsewhere"));
            }
            if !seen.insert(s) {
                return Err((span, "a macro annotation defining a symbol twice"));
            }
        }
        Ok(())
    }
}
