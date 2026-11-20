// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Statement lists, blocks and templates: definitions expanded as dotty's Namer enters them.

use super::{Desugar, Mode, Owner};
use crate::ast::{List, NodeId, Tree};
use crate::parse::language::imported_features;
use names::nme;

impl Desugar<'_> {
    /// The statements `l` with definitions expanded, then transformed; a new list unless empty
    /// (the oracle's `Expander.transformStats`).
    pub fn transform_stats(&mut self, l: List) -> List {
        if l.is_empty() {
            return l;
        }
        let items = self.ast.items(l).to_vec();
        let outer = self.cfg.features;
        let expanded: Vec<NodeId> = items
            .iter()
            .map(|&t| {
                self.enable_imported_features(t);
                let expanded = self.expand_stat(t);
                self.spread_doc_comment(t, expanded);
                expanded
            })
            .collect();
        self.cfg.features = outer;
        let merged = self.merge_companions(&expanded);
        let flat = self.flatten(&merged);
        self.register_classes(&flat);
        let mapped: Vec<NodeId> = flat
            .iter()
            .map(|&t| {
                self.enable_imported_features(t);
                self.transform_stat(t)
            })
            .collect();
        self.cfg.features = outer;
        self.list(&mapped)
    }

    /// Enables the features of `stat` for the statements after it if it is a language import.
    fn enable_imported_features(&mut self, stat: NodeId) {
        let features = imported_features(self.ast, self.names, stat);
        self.cfg.features.insert(features);
    }

    /// Transforms statement `t`, an expression of a class body as owned by a term (dotty's local
    /// dummy).
    fn transform_stat(&mut self, t: NodeId) -> NodeId {
        let import =
            matches!(self.ast.tree(t), Tree::Import { .. } | Tree::Export { .. } | Tree::PackageDef { .. });
        match self.owner.is_class() && self.ast.is_term(t) && !import {
            true => self.in_owner(Owner::Term { is_constructor: false }, |d| d.transform(t)),
            false => self.transform(t),
        }
    }

    /// Gives each definition of `expanded`, the expansion of `stat`, the doc comment of `stat`
    /// (dotty's `setDocstring(sym, origStat)` in `indexExpanded`).
    fn spread_doc_comment(&mut self, stat: NodeId, expanded: NodeId) {
        let Some(comment) = self.ast.doc_comment(stat) else { return };
        for t in self.flatten(&[expanded]) {
            let defines =
                matches!(self.ast.tree(t), Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. });
            if defines {
                self.ast.set_doc_comment(t, comment);
            }
        }
    }

    /// `stat` as Namer expands it before entering it.
    fn expand_stat(&mut self, stat: NodeId) -> NodeId {
        match *self.ast.tree(stat) {
            Tree::PatDef { .. } => self.pat_def(stat),
            Tree::ValDef { .. } => {
                let stat = self.check_modifiers(stat);
                self.val_def(stat)
            }
            Tree::DefDef { name, .. } if self.text(name) != nme::CONSTRUCTOR => {
                let stat = self.check_modifiers(stat);
                self.def_def(stat, false)
            }
            Tree::DefDef { .. } => self.check_modifiers(stat),
            Tree::TypeDef { rhs, .. } if !matches!(self.ast.tree(rhs), Tree::Template { .. }) => {
                let stat = self.check_modifiers(stat);
                self.type_def(stat)
            }
            Tree::TypeDef { .. } => {
                let stat = self.check_modifiers(stat);
                self.class_def(stat)
            }
            Tree::ModuleDef { .. } => {
                let stat = self.check_modifiers(stat);
                self.module_def(stat, false)
            }
            Tree::PackageDef { .. } => self.package_def(stat),
            Tree::ExtMethods { .. } => self.ext_methods(stat),
            _ => stat,
        }
    }

    pub fn transform_block(&mut self, id: NodeId) -> NodeId {
        let Tree::Block { stats, expr } = *self.ast.tree(id) else { unreachable!("a block") };
        let stats = self.transform_stats(stats);
        let expr = self.transform_opt(expr);
        self.ast.derived(id, Tree::Block { stats, expr })
    }

    /// The template `id` with its parts transformed in the modes the typer gives them.
    pub fn transform_template(&mut self, id: NodeId) -> NodeId {
        let Tree::Template { constr, parents_or_derived, self_, body, derived } = *self.ast.tree(id) else {
            unreachable!("a template")
        };
        let constr = self.transform(constr);
        let items = self.ast.items(parents_or_derived).to_vec();
        let (parents, derived_classes) = items.split_at(items.len() - derived.unwrap_or(0) as usize);
        let mut mapped: Vec<NodeId> = parents
            .iter()
            .map(|&p| match self.ast.tree(p) {
                Tree::Apply { .. } | Tree::TypeApply { .. } | Tree::Select { .. } | Tree::New { .. } => {
                    self.in_mode(Mode::Expr, |d| d.transform(p))
                }
                _ => self.in_mode(Mode::Type, |d| d.transform(p)),
            })
            .collect();
        let derived_list = self.list(derived_classes);
        let derived_list = self.in_mode(Mode::Type, |d| d.transform_list(derived_list));
        mapped.extend_from_slice(self.ast.items(derived_list));
        let derived = derived.map(|_| derived_list.len() as u32).filter(|&n| n > 0);
        let parents_or_derived = if mapped == items { parents_or_derived } else { self.list(&mapped) };
        let self_ = self_.map(|s| self.transform_self(s));
        let body = self.transform_stats(body);
        self.ast.derived(id, Tree::Template { constr, parents_or_derived, self_, body, derived })
    }

    fn transform_self(&mut self, id: NodeId) -> NodeId {
        let Tree::ValDef { name, tpt, rhs, mods } = *self.ast.tree(id) else { return id };
        let tpt = self.in_mode(Mode::Type, |d| d.transform(tpt));
        self.ast.derived(id, Tree::ValDef { name, tpt, rhs, mods })
    }
}
