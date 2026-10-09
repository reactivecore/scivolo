//! The references enum cases may make (dotty's `checkEnumCaseRefsLegal`).

use super::Typer;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use syntax::names::Name;

impl Typer<'_> {
    fn is_enum_case_def(&self, s: NodeId) -> bool {
        match *self.ast.tree(s) {
            Tree::ValDef { mods, .. } | Tree::TypeDef { mods, .. } => {
                self.ast.modifiers(mods).flags.is_all_of(f::ENUM.or(f::CASE))
            }
            _ => false,
        }
    }

    /// Checks that the enum cases among the statements `body` of an object refer neither by
    /// simple name to another of its members nor to `this` in their parents, constructors,
    /// constructor default getters and the parents of their anonymous classes.
    pub fn check_enum_case_refs(&mut self, body: &[NodeId]) {
        let cases: Vec<NodeId> = body.iter().copied().filter(|&s| self.is_enum_case_def(s)).collect();
        if cases.is_empty() {
            return;
        }
        let members: Vec<Name> = body
            .iter()
            .filter(|&&s| !self.is_enum_case_def(s))
            .filter_map(|&s| match *self.ast.tree(s) {
                Tree::ValDef { name, .. } | Tree::DefDef { name, .. } | Tree::TypeDef { name, .. } => {
                    Some(name.name)
                }
                _ => None,
            })
            .collect();
        let mut case_names = Vec::new();
        for &c in &cases {
            let roots = match *self.ast.tree(c) {
                Tree::ValDef { rhs: Some(r), .. } => {
                    let templates = self.ast.subtrees(r, &|a, t| matches!(a.tree(t), Tree::Template { .. }));
                    templates.into_iter().flat_map(|t| self.template_parents(t)).collect()
                }
                Tree::TypeDef { name, rhs, .. } => {
                    case_names.push(self.text(name.name).to_owned());
                    let mut roots = self.template_parents(rhs);
                    if let Tree::Template { constr, .. } = *self.ast.tree(rhs) {
                        roots.push(constr);
                    }
                    roots
                }
                _ => continue,
            };
            let bound: Vec<Name> = roots
                .iter()
                .flat_map(|&r| self.ast.subtrees(r, &|a, t| matches!(a.tree(t), Tree::ValDef { .. })))
                .filter_map(|v| match *self.ast.tree(v) {
                    Tree::ValDef { name, .. } => Some(name.name),
                    _ => None,
                })
                .collect();
            if roots.iter().any(|&r| self.refers_to_members(r, &members, &bound)) {
                self.unsupported(c, "reference to a companion member from an enum case");
            }
        }
        for &s in body {
            let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(s) else { continue };
            let text = self.text(name.name);
            if !case_names.iter().any(|c| tasty::module_name(text) == Some(c.as_str())) {
                continue;
            }
            let Tree::Template { body: stats, .. } = *self.ast.tree(rhs) else { continue };
            for d in self.ast.items(stats).to_vec() {
                let Tree::DefDef { name, rhs: Some(r), .. } = *self.ast.tree(d) else { continue };
                let getter = self.text(name.name).starts_with("$lessinit$greater$default$");
                if getter && self.refers_to_members(r, &members, &[]) {
                    self.unsupported(d, "reference to a companion member from an enum case");
                }
            }
        }
    }

    /// The parents of template `t`.
    fn template_parents(&self, t: NodeId) -> Vec<NodeId> {
        match *self.ast.tree(t) {
            Tree::Template { parents_or_derived, .. } => self.ast.items(parents_or_derived).to_vec(),
            _ => Vec::new(),
        }
    }

    /// Whether tree `r` has `this` or an identifier naming one of `members` but none of `bound`.
    fn refers_to_members(&self, r: NodeId, members: &[Name], bound: &[Name]) -> bool {
        self.ast.exists_subtree(r, &|a, t| match *a.tree(t) {
            Tree::Ident { name } => members.contains(&name.name) && !bound.contains(&name.name),
            Tree::This { qual: None } => true,
            _ => false,
        })
    }
}
