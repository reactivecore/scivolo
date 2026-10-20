// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Synthetic companions merged into explicit objects of the same name (the oracle's replay of
//! dotty's `Namer.mergeCompanionDefs`).

use super::Desugar;
use crate::ast::flags;
use crate::ast::{Attrs, List, NodeId, TName, Tree};

impl Desugar<'_> {
    /// The expansions of a statement list with each synthetic module class folded into the
    /// explicit one of the same name, and its module value dropped.
    pub fn merge_companions(&mut self, expanded: &[NodeId]) -> Vec<NodeId> {
        let mut groups: Vec<Vec<NodeId>> = expanded
            .iter()
            .map(|&t| match *self.ast.tree(t) {
                Tree::Thicket { trees } => self.ast.items(trees).to_vec(),
                _ => vec![t],
            })
            .collect();
        for (i, syn) in self.module_classes(&groups) {
            if !self.is_synthetic(syn) {
                continue;
            }
            let name = self.class_name(syn);
            let found = self
                .module_classes(&groups)
                .into_iter()
                .find(|&(j, t)| j != i && self.class_name(t) == name && !self.is_synthetic(t));
            let Some((j, expl)) = found else { continue };
            let merged = self.merged_companion(syn, expl);
            if let Some(k) = groups[j].iter().position(|&t| t == expl) {
                groups[j][k] = merged;
            }
            groups[i].retain(|&t| t != syn);
            let source_name = name.strip_suffix('$').unwrap_or(&name);
            let val_name = TName { is_type: false, name: self.names.intern(source_name) };
            if let Some(k) = groups[i].iter().position(|&t| {
                matches!(*self.ast.tree(t), Tree::ValDef { name, mods, .. }
                    if name == val_name && self.ast.modifiers(mods).is(flags::SYNTHETIC))
            }) {
                groups[i].remove(k);
            }
        }
        groups.concat()
    }

    fn module_classes(&self, groups: &[Vec<NodeId>]) -> Vec<(usize, NodeId)> {
        let mut out = Vec::new();
        for (i, g) in groups.iter().enumerate() {
            for &t in g {
                if let Tree::TypeDef { mods, .. } = *self.ast.tree(t)
                    && self.ast.modifiers(mods).is(flags::MODULE.to_type_flags())
                {
                    out.push((i, t));
                }
            }
        }
        out
    }

    fn is_synthetic(&self, t: NodeId) -> bool {
        self.ast.mods_of(t).is_some_and(|m| self.ast.modifiers(m).is(flags::SYNTHETIC))
    }

    fn class_name(&self, t: NodeId) -> String {
        self.text(self.name_of_def(t)).to_string()
    }

    /// Explicit module class `expl` with the body of synthetic `syn` (except `toString`) in front
    /// of its own, and the derived classes of `syn` if it has any.
    fn merged_companion(&mut self, syn: NodeId, expl: NodeId) -> NodeId {
        let template = |d: &Self, t: NodeId| match *d.ast.tree(t) {
            Tree::TypeDef { rhs, .. } => rhs,
            _ => unreachable!("a class"),
        };
        let (syn_t, expl_t) = (template(self, syn), template(self, expl));
        let Tree::Template { parents_or_derived: syn_pd, derived: syn_n, body: syn_body, .. } =
            *self.ast.tree(syn_t)
        else {
            unreachable!("a template")
        };
        let Tree::Template { constr, parents_or_derived, self_, body, derived } = *self.ast.tree(expl_t)
        else {
            unreachable!("a template")
        };
        let syn_derived = self.derived_of(syn_pd, syn_n);
        if !syn_derived.is_empty() && derived.is_some_and(|n| n > 0) {
            self.error(expl, "a class and its companion cannot both have `derives` clauses");
        }
        let items = self.ast.items(parents_or_derived).to_vec();
        let parents = &items[..items.len() - derived.unwrap_or(0) as usize];
        let (new_derived, n) = if syn_derived.is_empty() {
            (self.derived_of(parents_or_derived, derived), derived)
        } else {
            (syn_derived.clone(), Some(syn_derived.len() as u32))
        };
        let parents_or_derived = self.list(&[parents, &new_derived].concat());
        let mut stats: Vec<NodeId> = self
            .ast
            .items(syn_body)
            .iter()
            .copied()
            .filter(
                |&s| !matches!(*self.ast.tree(s), Tree::DefDef { name, .. } if self.text(name) == "toString"),
            )
            .collect();
        stats.extend_from_slice(self.ast.items(body));
        let body = self.list(&stats);
        let rhs =
            self.ast.derived(expl_t, Tree::Template { constr, parents_or_derived, self_, body, derived: n });
        let Tree::TypeDef { name, mods, .. } = *self.ast.tree(expl) else { unreachable!("a class") };
        let merged = self.ast.derived(expl, Tree::TypeDef { name, rhs, mods });
        if !syn_derived.is_empty() {
            self.ast.add_attrs(merged, Attrs::DERIVING_COMPANION.union(Attrs::DERIVES_OF_CLASS));
        }
        merged
    }

    fn derived_of(&self, parents_or_derived: List, derived: Option<u32>) -> Vec<NodeId> {
        let items = self.ast.items(parents_or_derived);
        items[items.len() - derived.unwrap_or(0) as usize..].to_vec()
    }
}
