//! Definitions and imports, their parts transformed under the owner and in the mode the typer
//! gives them.

use super::{Desugar, Mode, Owner};
use crate::ast::flags;
use crate::ast::{NodeId, Tree};

impl Desugar<'_> {
    /// The value, method, type, class or object definition `id` transformed.
    pub fn transform_def(&mut self, id: NodeId) -> NodeId {
        match *self.ast.tree(id) {
            Tree::ValDef { name, tpt, rhs, mods } => {
                self.in_owner(Owner::Term { is_constructor: false }, |d| {
                    let tpt = d.in_mode(Mode::Type, |d| d.transform(tpt));
                    let rhs = d.in_mode(Mode::Expr, |d| d.transform_opt(rhs));
                    d.ast.derived(id, Tree::ValDef { name, tpt, rhs, mods })
                })
            }
            Tree::DefDef { name, paramss, tpt, rhs, mods } => {
                let owner = Owner::Term { is_constructor: self.text(name) == "<init>" };
                self.in_owner(owner, |d| {
                    let paramss = d.transform_clauses(paramss);
                    let tpt = d.in_mode(Mode::Type, |d| d.transform(tpt));
                    let rhs = d.in_mode(Mode::Expr, |d| d.transform_opt(rhs));
                    d.ast.derived(id, Tree::DefDef { name, paramss, tpt, rhs, mods })
                })
            }
            Tree::TypeDef { name, rhs, mods } => {
                let rhs = if matches!(self.ast.tree(rhs), Tree::Template { .. }) {
                    let is_trait = self.ast.modifiers(mods).is(flags::TRAIT);
                    let owner = Owner::Class { is_trait, is_package_object: false };
                    self.in_owner(owner, |d| d.transform_template(rhs))
                } else {
                    self.in_mode(Mode::Type, |d| d.transform(rhs))
                };
                self.ast.derived(id, Tree::TypeDef { name, rhs, mods })
            }
            Tree::ModuleDef { name, imp, mods } => {
                let is_package_object = self.owner == Owner::Package
                    && (self.ast.modifiers(mods).is(flags::PACKAGE) || self.text(name).ends_with("$package"));
                let owner = Owner::Class { is_trait: false, is_package_object };
                let imp = self.in_owner(owner, |d| d.transform_template(imp));
                self.ast.derived(id, Tree::ModuleDef { name, imp, mods })
            }
            _ => unreachable!("a definition"),
        }
    }

    /// The import or export `id` with its selectors' bounds transformed as types.
    pub fn transform_import(&mut self, id: NodeId) -> NodeId {
        let (Tree::Import { expr, selectors } | Tree::Export { expr, selectors }) = *self.ast.tree(id) else {
            unreachable!("an import")
        };
        let expr = self.transform(expr);
        let items = self.ast.items(selectors).to_vec();
        let mapped: Vec<NodeId> = items
            .iter()
            .map(|&s| {
                let Tree::ImportSelector { imported, renamed, bound } = *self.ast.tree(s) else { return s };
                let bound = self.in_mode(Mode::Type, |d| d.transform_opt(bound));
                self.ast.derived(s, Tree::ImportSelector { imported, renamed, bound })
            })
            .collect();
        let selectors = if mapped == items { selectors } else { self.list(&mapped) };
        let tree = match self.ast.tree(id) {
            Tree::Import { .. } => Tree::Import { expr, selectors },
            _ => Tree::Export { expr, selectors },
        };
        self.ast.derived(id, tree)
    }
}
