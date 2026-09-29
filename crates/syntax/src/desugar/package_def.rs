//! Top-level definitions wrapped in an object `<file>$package` (dotty's `packageDef`).

use super::Desugar;
use crate::ast::flags;
use crate::ast::span::Span;
use crate::ast::{NodeId, TName, Tree};

impl Desugar<'_> {
    /// Package `pdef` with the definitions that cannot be top-level moved into the package
    /// object of the source file.
    pub fn package_def(&mut self, pdef: NodeId) -> NodeId {
        let Tree::PackageDef { pid, stats } = *self.ast.tree(pdef) else { return pdef };
        self.check_package_name(pid);
        let stats = self.ast.items(stats).to_vec();
        let wrapped_types: Vec<TName> = stats
            .iter()
            .filter(|&&s| matches!(self.ast.tree(s), Tree::TypeDef { .. }) && self.is_top_level_def(s))
            .filter_map(|&s| match *self.ast.tree(s) {
                Tree::TypeDef { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        let (nested, top): (Vec<NodeId>, Vec<NodeId>) = stats.iter().partition(|&&s| {
            self.is_top_level_def(s)
                || matches!(*self.ast.tree(s), Tree::ModuleDef { name, .. }
                    if wrapped_types.contains(&TName { is_type: true, ..name }))
        });
        if nested.is_empty() {
            return pdef;
        }
        let file = self.source.name.rsplit('/').next().unwrap_or(&self.source.name);
        let stem = file.rfind('.').map_or("", |i| &file[..i]);
        let name = self.term(&format!("{stem}$package"));
        let constr = self.empty_constructor();
        let parents = self.list(&[]);
        let body = self.list(&nested);
        let imp = self.ast.node(Tree::Template {
            constr,
            parents_or_derived: parents,
            self_: None,
            body,
            derived: None,
        });
        let mods = self.mods(flags::SYNTHETIC);
        let grouped = self.ast.node(Tree::ModuleDef { name, imp, mods });
        let stats = self.list(&[&top[..], &[grouped]].concat());
        self.ast.derived(pdef, Tree::PackageDef { pid, stats })
    }

    /// `def <init>()` with no parameters (dotty's `emptyConstructor`).
    fn empty_constructor(&mut self) -> NodeId {
        let name = self.term("<init>");
        let paramss = self.ast.clauses(&[]);
        let tpt = self.type_tree();
        let mods = self.mods(flags::EMPTY);
        self.ast.node(Tree::DefDef { name, paramss, tpt, rhs: None, mods })
    }

    fn is_top_level_def(&self, stat: NodeId) -> bool {
        let given_or_implicit = |m| self.ast.modifiers(m).is_one_of(flags::GIVEN_OR_IMPLICIT);
        match *self.ast.tree(stat) {
            Tree::ValDef { .. }
            | Tree::PatDef { .. }
            | Tree::DefDef { .. }
            | Tree::Export { .. }
            | Tree::ExtMethods { .. } => true,
            Tree::ModuleDef { mods, .. } => given_or_implicit(mods),
            Tree::TypeDef { rhs, mods, .. } => {
                !matches!(self.ast.tree(rhs), Tree::Template { .. }) || given_or_implicit(mods)
            }
            _ => false,
        }
    }

    /// A warning for each part of package name `pid` that the class path encodes.
    fn check_package_name(&mut self, pid: NodeId) {
        let (name, span, qual) = match *self.ast.tree(pid) {
            Tree::Ident { name } => (name, self.ast.span(pid), None),
            Tree::Select { qualifier, name } => {
                let span = self.ast.span(pid);
                let name_span =
                    if span.exists() { Span::new(span.point(), span.end(), span.point()) } else { span };
                (name, name_span, Some(qualifier))
            }
            _ => return,
        };
        let encoded = self.text(name).chars().any(|c| !crate::scan::chars::is_java_identifier_part(c));
        if !span.is_synthetic() && encoded {
            let msg = format!(
                "The package name `{}` will be encoded on the classpath, and can lead to undefined behaviour.",
                self.text(name)
            );
            self.diags.warning_at(span.start(), span.end(), span.point(), msg);
        }
        if let Some(q) = qual {
            self.check_package_name(q);
        }
    }
}
