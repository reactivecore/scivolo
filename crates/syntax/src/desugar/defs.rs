// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Definitions and imports, their parts transformed under the owner and in the mode the typer
//! gives them.

use super::{Desugar, Mode, Owner};
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, Tree};
use names::nme;

impl Desugar<'_> {
    /// The value, method, type, class or object definition `id` transformed.
    pub fn transform_def(&mut self, id: NodeId) -> NodeId {
        let id = self.with_annotation_args(id);
        match *self.ast.tree(id) {
            Tree::ValDef { name, tpt, rhs, mods } => self.in_term(false, |d| {
                let tpt = d.in_mode(Mode::Type, |d| d.transform(tpt));
                let rhs = d.in_mode(Mode::Expr, |d| d.transform_opt(rhs));
                d.ast.derived(id, Tree::ValDef { name, tpt, rhs, mods })
            }),
            Tree::DefDef { name, paramss, tpt, rhs, mods } => {
                let is_constructor = self.text(name) == nme::CONSTRUCTOR;
                self.in_term(is_constructor, |d| {
                    let paramss = d.transform_clauses(paramss);
                    let tpt = d.in_mode(Mode::Type, |d| d.transform(tpt));
                    let rhs = d.in_mode(Mode::Expr, |d| d.transform_opt(rhs));
                    d.ast.derived(id, Tree::DefDef { name, paramss, tpt, rhs, mods })
                })
            }
            Tree::TypeDef { name, rhs, mods } => {
                let rhs = if matches!(self.ast.tree(rhs), Tree::Template { .. }) {
                    let m = self.ast.modifiers(mods);
                    let is_trait = m.is(flags::TRAIT);
                    let text = self.text(name);
                    let is_package_object = m.is(flags::MODULE.to_type_flags())
                        && self.owner == Owner::Package
                        && (text == "package$" || text.ends_with("$package$"));
                    let owner = Owner::Class { is_trait, is_package_object };
                    let class = self.class_sym(id);
                    self.in_class(class, owner, |d| d.transform_template(rhs))
                } else {
                    self.in_mode(Mode::Type, |d| d.transform(rhs))
                };
                self.ast.derived(id, Tree::TypeDef { name, rhs, mods })
            }
            Tree::ModuleDef { name, imp, mods } => {
                let is_package_object = self.owner == Owner::Package
                    && (self.ast.modifiers(mods).is(flags::PACKAGE)
                        || self.text(name).ends_with(nme::TOPLEVEL_SUFFIX));
                let owner = Owner::Class { is_trait: false, is_package_object };
                let module_class =
                    if self.ast.modifiers(mods).is(flags::PACKAGE) { "package" } else { self.text(name) };
                let full_name = self.child_full_name(&format!("{module_class}$"));
                let class = self.new_class(full_name, Vec::new(), true, None);
                let imp = self.in_class(class, owner, |d| d.transform_template(imp));
                self.ast.derived(id, Tree::ModuleDef { name, imp, mods })
            }
            _ => unreachable!("a definition"),
        }
    }

    /// Definition `id` with the arguments of its annotations transformed as expressions.
    fn with_annotation_args(&mut self, id: NodeId) -> NodeId {
        let tree = self.ast.tree(id).clone();
        let (Tree::ValDef { mods, .. }
        | Tree::DefDef { mods, .. }
        | Tree::TypeDef { mods, .. }
        | Tree::ModuleDef { mods, .. }) = tree
        else {
            return id;
        };
        let m = self.ast.modifiers(mods).clone();
        let items = self.ast.items(m.annotations).to_vec();
        let mapped: Vec<NodeId> = items.iter().map(|&a| self.annotation_args(a)).collect();
        if mapped == items {
            return id;
        }
        let annotations = self.list(&mapped);
        let mods = self.ast.mods(Modifiers { annotations, ..m });
        let tree = match tree {
            Tree::ValDef { name, tpt, rhs, .. } => Tree::ValDef { name, tpt, rhs, mods },
            Tree::DefDef { name, paramss, tpt, rhs, .. } => Tree::DefDef { name, paramss, tpt, rhs, mods },
            Tree::TypeDef { name, rhs, .. } => Tree::TypeDef { name, rhs, mods },
            Tree::ModuleDef { name, imp, .. } => Tree::ModuleDef { name, imp, mods },
            _ => unreachable!("a definition"),
        };
        self.ast.derived(id, tree)
    }

    /// Annotation `a` with its arguments transformed as expressions.
    fn annotation_args(&mut self, a: NodeId) -> NodeId {
        let Tree::Apply { fun, args } = *self.ast.tree(a) else { return a };
        let new_fun = self.annotation_args(fun);
        let new_args = self.in_mode(Mode::Expr, |d| d.transform_list(args));
        if (new_fun, new_args) == (fun, args) {
            return a;
        }
        self.ast.derived(a, Tree::Apply { fun: new_fun, args: new_args })
    }

    /// The import or export `id` with its selectors' bounds transformed as types.
    pub fn transform_import(&mut self, id: NodeId) -> NodeId {
        let (Tree::Import { expr, selectors } | Tree::Export { expr, selectors, .. }) = *self.ast.tree(id)
        else {
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
        let tree = match *self.ast.tree(id) {
            Tree::Export { ext, .. } => Tree::Export { expr, selectors, ext },
            _ => Tree::Import { expr, selectors },
        };
        self.ast.derived(id, tree)
    }
}
