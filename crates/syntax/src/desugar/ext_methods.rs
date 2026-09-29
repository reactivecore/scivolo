//! Extensions as methods with the extension's parameters in front (dotty's `extMethods` and
//! `extMethod`).

use super::Desugar;
use crate::ast::flags;
use crate::ast::{List, NodeId, Tree};

impl Desugar<'_> {
    /// The methods of extension `ext`, exports kept.
    pub fn ext_methods(&mut self, ext: NodeId) -> NodeId {
        let Tree::ExtMethods { paramss, methods } = *self.ast.tree(ext) else { return ext };
        let ext_paramss = self.ast.clause_items(paramss).to_vec();
        let methods = self.ast.items(methods).to_vec();
        let expanded: Vec<NodeId> = methods
            .into_iter()
            .map(|m| match self.ast.tree(m) {
                Tree::Export { .. } => m,
                Tree::DefDef { .. } => {
                    let m = self.ext_method(m, &ext_paramss);
                    self.def_def(m)
                }
                _ => self.ast.node(Tree::Empty),
            })
            .collect();
        self.flat_tree(&expanded)
    }

    fn ext_method(&mut self, mdef: NodeId, ext_paramss: &[List]) -> NodeId {
        let Tree::DefDef { name, paramss, tpt, rhs, mods } = *self.ast.tree(mdef) else { return mdef };
        let own = self.ast.clause_items(paramss).to_vec();
        let clauses = if self.text(name).ends_with(':') {
            self.right_assoc_params(mdef, ext_paramss, &own)
        } else {
            [ext_paramss, &own].concat()
        };
        let name = self.normalize_name(name, tpt);
        let paramss = self.ast.clauses(&clauses);
        let mdef = self.ast.derived(mdef, Tree::DefDef { name, paramss, tpt, rhs, mods });
        let Some(m) = self.ast.mods_of(mdef) else { return mdef };
        let m = self.ast.modifiers(m).clone().or(flags::EXTENSION_METHOD);
        self.ast.with_mods(mdef, m)
    }

    /// The clauses of right-associative extension method `mdef`: the extension's leading type
    /// and using clauses, the method's type clauses, its first parameter, the rest of the
    /// extension's clauses, the rest of the method's.
    fn right_assoc_params(&mut self, mdef: NodeId, ext_paramss: &[List], own: &[List]) -> Vec<List> {
        let right_ty = own.iter().take_while(|&&l| self.is_type_clause(l)).count();
        let (right_ty_params, rest) = own.split_at(right_ty);
        let fallback = [ext_paramss, own].concat();
        let Some((&right_param, rest)) = rest.split_first() else { return fallback };
        let first = self.ast.items(right_param).to_vec();
        let Some(&vparam) = first.first() else { return fallback };
        if !matches!(self.ast.tree(vparam), Tree::ValDef { .. }) {
            return fallback;
        }
        let given = self.ast.mods_of(vparam).is_some_and(|m| self.ast.modifiers(m).is(flags::GIVEN));
        if first.len() != 1 || given {
            if !given {
                self.error(mdef, "right-associative extension method must start with a single parameter, consider a tupled parameter instead");
            }
            return fallback;
        }
        let leading =
            ext_paramss.iter().take_while(|&&l| self.is_type_clause(l) || self.is_using_clause(l)).count();
        let (left_leading, left_rest) = ext_paramss.split_at(leading);
        let names: Vec<_> = own
            .iter()
            .flat_map(|&l| self.ast.items(l).to_vec())
            .filter_map(|p| match *self.ast.tree(p) {
                Tree::ValDef { name, .. } | Tree::TypeDef { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        for &l in left_rest {
            for t in self.ast.items(l).to_vec() {
                for r in self.ast.subtrees(
                    t,
                    &|ast, n| matches!(*ast.tree(n), Tree::Ident { name } if names.contains(&name)),
                ) {
                    let Tree::Ident { name } = *self.ast.tree(r) else { continue };
                    let msg = format!(
                        "right-associative extension method cannot have a forward reference to {}",
                        self.text(name)
                    );
                    self.error(r, &msg);
                }
            }
        }
        [left_leading, right_ty_params, &[right_param], left_rest, rest].concat()
    }

    fn is_using_clause(&self, l: List) -> bool {
        self.ast
            .items(l)
            .first()
            .and_then(|&p| self.ast.mods_of(p))
            .is_some_and(|m| self.ast.modifiers(m).is(flags::GIVEN))
    }
}
