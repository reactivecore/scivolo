//! Generators of `for` comprehensions: their functions, filters and patterns.

use super::Desugar;
use super::case_lambda::MatchCheck;
use super::exprs::MAX_TUPLE_ARITY;
use crate::ast::constant::Constant;
use crate::ast::flags;
use crate::ast::tree::GenCheckMode;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// The function `pat => body` of generator `gen_`: a lambda for a variable pattern, a
    /// pattern-matching lambda otherwise.
    pub fn make_lambda(&mut self, gen_: NodeId, body: NodeId) -> NodeId {
        let Tree::GenFrom { pat, check_mode, .. } = *self.ast.tree(gen_) else { unreachable!("a generator") };
        if check_mode != GenCheckMode::FilterAlways
            && let Some((named, tpt)) = self.id_pattern(pat)
        {
            let span = self.ast.span(pat);
            let mods = self.mods(flags::PARAM);
            let param = self.derived_val_def(span, named, tpt, None, mods);
            let args = self.list(&[param]);
            return self.ast.node(Tree::Function { args, body, mods: None, wildcard: false });
        }
        let check = match check_mode {
            GenCheckMode::Check | GenCheckMode::CheckAndFilter => MatchCheck::GenFrom,
            _ => MatchCheck::Unchecked,
        };
        let pat = match self.ast.tree(pat).clone() {
            Tree::Tuple { trees } if trees.len() > MAX_TUPLE_ARITY => {
                let pats = self.ast.items(trees).to_vec();
                let pats: Vec<NodeId> = pats.into_iter().map(|p| self.unchecked_bind(p)).collect();
                let trees = self.list(&pats);
                self.ast.node(Tree::Tuple { trees })
            }
            _ => pat,
        };
        let case = self.ast.node(Tree::CaseDef { pat, guard: None, body });
        self.make_case_lambda(&[case], check)
    }

    /// `x @ (y: T @unchecked)` for `x @ (y: T)`.
    fn unchecked_bind(&mut self, p: NodeId) -> NodeId {
        let Tree::Bind { name, body: typed, mods } = *self.ast.tree(p) else { return p };
        let Tree::Typed { expr, tpt } = *self.ast.tree(typed) else { return p };
        let annot = self.new_annot("scala.unchecked");
        let annotated = self.ast.node(Tree::Annotated { arg: tpt, annot });
        let typed = self.ast.derived(typed, Tree::Typed { expr, tpt: annotated });
        self.ast.derived(p, Tree::Bind { name, body: typed, mods })
    }

    /// Whether `pat` binds a `given` below its top.
    pub fn is_nested_given_pattern(&self, pat: NodeId) -> bool {
        let pat = match *self.ast.tree(pat) {
            Tree::Bind { body, .. } => body,
            _ => pat,
        };
        self.ast.exists_subtree(
            pat,
            &|ast, n| matches!(*ast.tree(n), Tree::Bind { mods, .. } if ast.modifiers(mods).is(flags::GIVEN)),
        )
    }

    /// `pat` as a pattern binding a name, and a reference to that name (dotty's `makeIdPat`).
    pub fn make_id_pat(&mut self, pat: NodeId) -> (NodeId, NodeId) {
        match self.ast.tree(pat).clone() {
            Tree::Bind { name, body, mods } if self.text(name) == "_" => {
                let given_type = match *self.ast.tree(body) {
                    Tree::Typed { expr, tpt }
                        if self.ast.modifiers(mods).is(flags::GIVEN) && self.is_wildcard_ident(expr) =>
                    {
                        Some(tpt)
                    }
                    _ => None,
                };
                let name = match given_type {
                    Some(tpt) => self.invent_given_name(tpt),
                    None => TName { name: self.fresh.unique(self.names), is_type: false },
                };
                let bind = self.ast.derived(pat, Tree::Bind { name, body, mods });
                (bind, self.ident(name))
            }
            Tree::Bind { name, .. } => (pat, self.ident(name)),
            Tree::Ident { .. } if self.is_var_pattern(pat) && !self.is_wildcard_ident(pat) => (pat, pat),
            Tree::Typed { expr, .. }
                if matches!(self.ast.tree(expr), Tree::Ident { .. })
                    && self.is_var_pattern(expr)
                    && !self.is_wildcard_ident(expr) =>
            {
                (pat, expr)
            }
            _ => {
                let name = TName { name: self.fresh.unique(self.names), is_type: false };
                let mods = self.mods(flags::EMPTY);
                let bind = self.ast.node(Tree::Bind { name, body: pat, mods });
                (bind, self.ident(name))
            }
        }
    }

    /// `rhs.withFilter { case pat => true; case _ => false }`.
    fn make_pat_filter(&mut self, rhs: NodeId, pat: NodeId) -> NodeId {
        let yes = self.literal(Constant::Boolean(true));
        let case1 = self.ast.node(Tree::CaseDef { pat, guard: None, body: yes });
        let wildcard = self.term("_");
        let wildcard = self.ident(wildcard);
        let no = self.literal(Constant::Boolean(false));
        let case2 = self.ast.node(Tree::CaseDef { pat: wildcard, guard: None, body: no });
        let with_filter = self.term("withFilter");
        let select = self.select(rhs, with_filter);
        let lambda = self.make_case_lambda(&[case1, case2], MatchCheck::Unchecked);
        self.apply(select, &[lambda])
    }

    /// `rhs.name` of generator `gen_`, filtering the rhs unless its pattern needs no filter.
    pub fn rhs_select(&mut self, gen_: NodeId, name: TName) -> NodeId {
        let Tree::GenFrom { pat, expr, check_mode } = *self.ast.tree(gen_) else {
            unreachable!("a generator")
        };
        let no_filter = match check_mode {
            GenCheckMode::FilterAlways => false,
            GenCheckMode::FilterNow | GenCheckMode::CheckAndFilter => {
                self.is_var_binding(pat) || self.is_irrefutable(pat, expr)
            }
            GenCheckMode::Check | GenCheckMode::Ignore | GenCheckMode::Filtered => true,
        };
        let rhs = if no_filter { expr } else { self.make_pat_filter(expr, pat) };
        self.select(rhs, name)
    }

    /// Mark `apply`, the last call of a `for`, when its `map` only returns the pattern.
    pub fn mark_trailing_map(
        &mut self,
        apply: NodeId,
        gen_: NodeId,
        select: TName,
        map: TName,
        body: NodeId,
    ) {
        let Tree::GenFrom { pat, check_mode, .. } = *self.ast.tree(gen_) else { unreachable!("a generator") };
        let unit_body =
            matches!(self.ast.tree(self.unsplice(body)), Tree::Tuple { trees } if trees.is_empty());
        if self.version.enables_better_fors()
            && select == map
            && check_mode != GenCheckMode::Filtered
            && (self.deep_equals(pat, body) || unit_body)
        {
            self.ast.add_attrs(apply, Attrs::TRAILING_FOR_MAP);
        }
    }

    fn deep_equals(&self, t1: NodeId, t2: NodeId) -> bool {
        match (self.ast.tree(self.unsplice(t1)), self.ast.tree(self.unsplice(t2))) {
            (Tree::Ident { name: n1 }, Tree::Ident { name: n2 }) => n1 == n2,
            (Tree::Tuple { trees: ts1 }, Tree::Tuple { trees: ts2 }) => {
                let (ts1, ts2) = (self.ast.items(*ts1), self.ast.items(*ts2));
                ts1.len() == ts2.len() && ts1.iter().zip(ts2).all(|(&a, &b)| self.deep_equals(a, b))
            }
            _ => false,
        }
    }
}
