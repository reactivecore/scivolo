//! Pattern shapes and the variables a pattern binds.

use super::Desugar;
use crate::ast::flags;
use crate::ast::{Attrs, NodeId, Tree};
use crate::names::is_var_pattern_name;

impl Desugar<'_> {
    /// `t` without `Parens` around it (dotty's untyped `unsplice`).
    pub fn unsplice(&self, t: NodeId) -> NodeId {
        match *self.ast.tree(t) {
            Tree::Parens { t } => self.unsplice(t),
            _ => t,
        }
    }

    pub fn is_var_pattern(&self, t: NodeId) -> bool {
        let t = self.unsplice(t);
        match *self.ast.tree(t) {
            Tree::Ident { name } => {
                is_var_pattern_name(self.text(name)) && !self.ast.attrs(t).contains(Attrs::BACKQUOTED)
            }
            _ => false,
        }
    }

    pub fn is_wildcard_ident(&self, t: NodeId) -> bool {
        matches!(*self.ast.tree(t), Tree::Ident { name } if self.text(name) == "_")
    }

    /// The name and type of `x`, `x: T` or `_: T` (dotty's `IdPattern`).
    pub fn id_pattern(&mut self, pat: NodeId) -> Option<(NodeId, NodeId)> {
        match *self.ast.tree(pat) {
            Tree::Ident { .. } if !self.is_wildcard_ident(pat) => Some((pat, self.type_tree())),
            Tree::Typed { expr, tpt } if matches!(self.ast.tree(expr), Tree::Ident { .. }) => {
                Some((expr, tpt))
            }
            _ => None,
        }
    }

    /// Whether generator pattern `pat` is `x`, `x: T` or `given T`.
    pub fn is_var_binding(&mut self, pat: NodeId) -> bool {
        match *self.ast.tree(pat) {
            Tree::Bind { body, mods, .. } if self.ast.modifiers(mods).is(flags::GIVEN) => {
                self.is_var_binding(body)
            }
            _ => self.id_pattern(pat).is_some(),
        }
    }

    /// Whether `pat` matches every value of `rhs`, as far as the syntax shows.
    pub fn is_irrefutable(&self, pat: NodeId, rhs: NodeId) -> bool {
        match *self.ast.tree(pat) {
            Tree::Bind { body, .. } | Tree::Parens { t: body } => self.is_irrefutable(body, rhs),
            Tree::Tuple { trees } => self.matches_tuple(self.ast.items(trees), rhs),
            _ => self.is_var_pattern(pat),
        }
    }

    fn matches_tuple(&self, pats: &[NodeId], rhs: NodeId) -> bool {
        match *self.ast.tree(rhs) {
            Tree::Tuple { trees } => {
                let trees = self.ast.items(trees);
                pats.len() == trees.len() && pats.iter().zip(trees).all(|(&p, &t)| self.is_irrefutable(p, t))
            }
            Tree::Parens { t } | Tree::Block { expr: Some(t), .. } | Tree::CaseDef { body: t, .. } => {
                self.matches_tuple(pats, t)
            }
            Tree::If { thenp, elsep, .. } => {
                self.matches_tuple(pats, thenp) && elsep.is_some_and(|e| self.matches_tuple(pats, e))
            }
            Tree::Match { cases, .. } => self.ast.items(cases).iter().all(|&c| self.matches_tuple(pats, c)),
            Tree::Throw { .. } => true,
            _ => false,
        }
    }

    /// The variables `pat` binds with their types, each name once (dotty's `getVariables`);
    /// `add_given` decides on a `given` binding.
    pub fn get_variables(
        &mut self,
        pat: NodeId,
        add_given: &mut dyn FnMut(&mut Self, NodeId) -> bool,
    ) -> Vec<(NodeId, NodeId)> {
        let mut vars = Vec::new();
        self.collect_variables(pat, add_given, &mut vars);
        vars
    }

    fn add_variable(&self, vars: &mut Vec<(NodeId, NodeId)>, named: NodeId, tpt: NodeId) {
        let name = self.name_of(named);
        if !name.is_type && !vars.iter().any(|&(n, _)| self.name_of(n) == name) {
            vars.push((named, tpt));
        }
    }

    pub fn name_of(&self, named: NodeId) -> crate::ast::TName {
        match *self.ast.tree(named) {
            Tree::Ident { name } | Tree::Bind { name, .. } => name,
            _ => unreachable!("a named tree"),
        }
    }

    fn collect_variables(
        &mut self,
        t: NodeId,
        add_given: &mut dyn FnMut(&mut Self, NodeId) -> bool,
        vars: &mut Vec<(NodeId, NodeId)>,
    ) {
        match *self.ast.tree(t) {
            Tree::Bind { name, body, mods } => {
                let given = self.ast.modifiers(mods).is(flags::GIVEN);
                let typed = match *self.ast.tree(body) {
                    Tree::Typed { expr, tpt } => Some((expr, tpt)),
                    _ => None,
                };
                if self.text(name) == "_" {
                    if given
                        && let Some((_, tpt)) = typed
                        && add_given(self, t)
                    {
                        self.add_variable(vars, t, tpt);
                    }
                    self.collect_variables(body, add_given, vars);
                } else if let Some((expr, tpt)) = typed {
                    if !given || add_given(self, t) {
                        self.add_variable(vars, t, tpt);
                    }
                    self.collect_variables(expr, add_given, vars);
                } else {
                    let tpt = self.type_tree();
                    self.add_variable(vars, t, tpt);
                    self.collect_variables(body, add_given, vars);
                }
            }
            Tree::Typed { expr, tpt }
                if matches!(self.ast.tree(expr), Tree::Ident { .. })
                    && self.is_var_pattern(expr)
                    && !self.is_wildcard_ident(expr)
                    && !self.is_wildcard_star_arg(t) =>
            {
                self.add_variable(vars, expr, tpt)
            }
            Tree::Ident { .. } if self.is_var_pattern(t) && !self.is_wildcard_ident(t) => {
                let tpt = self.type_tree();
                self.add_variable(vars, t, tpt);
            }
            Tree::Apply { args: l, .. }
            | Tree::Tuple { trees: l }
            | Tree::Thicket { trees: l }
            | Tree::InterpolatedString { segments: l, .. } => {
                for t in self.ast.items(l).to_vec() {
                    self.collect_variables(t, add_given, vars);
                }
            }
            Tree::Typed { expr: t, .. }
            | Tree::NamedArg { arg: t, .. }
            | Tree::Annotated { arg: t, .. }
            | Tree::PrefixOp { od: t, .. }
            | Tree::Parens { t } => self.collect_variables(t, add_given, vars),
            Tree::InfixOp { left, right, .. } => {
                self.collect_variables(left, add_given, vars);
                self.collect_variables(right, add_given, vars);
            }
            Tree::Block { stats, expr: Some(expr) } if stats.is_empty() => {
                self.collect_variables(expr, add_given, vars);
            }
            Tree::Alternative { trees } => {
                for alt in self.ast.items(trees).to_vec() {
                    for (v, _) in self.get_variables(alt, add_given) {
                        self.error(v, "Illegal variable <none> in pattern alternative");
                    }
                }
            }
            Tree::Quote { body } => {
                for sp in self.ast.subtrees(body, &|ast, n| matches!(ast.tree(n), Tree::SplicePattern { .. }))
                {
                    let Tree::SplicePattern { body } = *self.ast.tree(sp) else { continue };
                    self.collect_variables(body, add_given, vars);
                }
            }
            _ => {}
        }
    }

    /// Whether `t` is `e: _*` or `e*` (dotty's `isWildcardStarArg` before typing).
    fn is_wildcard_star_arg(&self, t: NodeId) -> bool {
        match *self.ast.tree(t) {
            Tree::Bind { body, .. } => self.is_wildcard_star_arg(body),
            Tree::Typed { expr, tpt } => {
                matches!(*self.ast.tree(expr), Tree::Ident { name } if !name.is_type && self.text(name) == "_*")
                    || matches!(*self.ast.tree(tpt), Tree::Ident { name } if name.is_type && self.text(name) == "_*")
            }
            Tree::NamedArg { arg, .. } => self.is_wildcard_star_arg(arg),
            _ => false,
        }
    }
}
