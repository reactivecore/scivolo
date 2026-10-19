// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Pattern definitions `val p = e` as definitions of the variables of `p` (dotty's `patDef` and
//! `makePatDef`).

use super::Desugar;
use crate::ast::flags;
use crate::ast::span::Span;
use crate::ast::{Attrs, NodeId, TName, Tree};

/// How the right-hand side of a pattern definition is taken apart.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lowering {
    /// Used as it is: a tuple for a tuple of variables.
    SimpleTuple,
    /// Matched against the pattern, bound as a whole.
    MatchableTuple,
    /// Matched against the pattern, the tuple of its variables returned.
    General,
}

const GIVEN_IN_VAL: &str = "given patterns are not allowed in a val definition,\nplease bind to an identifier and use an alias given.";

impl Desugar<'_> {
    /// `val p1, ..., pN: T = E` as the definitions of `makePatDef` for each pattern.
    pub fn pat_def(&mut self, id: NodeId) -> NodeId {
        let Tree::PatDef { pats, tpt, rhs, mods } = *self.ast.tree(id) else {
            unreachable!("a pattern definition")
        };
        let mut pats = self.ast.items(pats).to_vec();
        if self.is_enum_case(id) {
            let lookups = self.defines_enum_lookups(id);
            let m = self.ast.modifiers(mods).clone();
            let last = pats.len().saturating_sub(1);
            let cases: Vec<NodeId> = pats
                .iter()
                .enumerate()
                .map(|(i, &p)| {
                    let Tree::Ident { name } = *self.ast.tree(p) else { return p };
                    let s = self.ast.span(p);
                    let span = Span::new(s.start(), s.end(), s.start());
                    self.expand_simple_enum_case(name, m.clone(), lookups && i == last, span)
                })
                .collect();
            return self.flat_tree(&cases);
        }
        if !matches!(self.ast.tree(tpt), Tree::TypeTree | Tree::Empty) {
            for p in &mut pats {
                *p = self.ast.node(Tree::Typed { expr: *p, tpt });
            }
        }
        let defs: Vec<NodeId> = pats.into_iter().map(|p| self.make_pat_def(id, p, rhs)).collect();
        self.flat_tree(&defs)
    }

    /// The definitions of the variables `pat` binds in `rhs`; `original` is the `PatDef` or
    /// generator alias.
    pub fn make_pat_def(&mut self, original: NodeId, pat: NodeId, rhs: Option<NodeId>) -> NodeId {
        let (mods, given_allowed) = match *self.ast.tree(original) {
            Tree::PatDef { mods, .. } => (mods, false),
            _ => match *self.ast.tree(pat) {
                Tree::Bind { mods, .. } | Tree::ValDef { mods, .. } | Tree::DefDef { mods, .. } => {
                    (mods, true)
                }
                _ => (self.mods(flags::EMPTY), true),
            },
        };
        let span = self.ast.span(original);
        let rhs = rhs.unwrap_or_else(|| self.ast.node(Tree::Empty));
        if let Some((id, tpt)) = self.id_pattern(pat) {
            let id = if self.is_wildcard_ident(id) {
                let name = TName { name: self.fresh.next(self.names, "_$"), is_type: false };
                self.ast.derived(id, Tree::Ident { name })
            } else {
                id
            };
            return self.derived_val_def(span, id, tpt, Some(rhs), mods);
        }
        let (lowering, variables) = self.lowering(pat, rhs, given_allowed);
        let lowered = match lowering {
            Lowering::SimpleTuple => rhs,
            Lowering::MatchableTuple => {
                let tmp = TName { name: self.fresh.unique(self.names), is_type: false };
                let tupled = self.ident(tmp);
                self.ast.add_attrs(tupled, Attrs::FOR_ARTIFACT);
                let bind_mods = self.mods(flags::EMPTY);
                let bind = self.ast.node(Tree::Bind { name: tmp, body: pat, mods: bind_mods });
                self.single_case_match(rhs, bind, tupled)
            }
            Lowering::General => {
                let ids: Vec<NodeId> = variables.iter().map(|&(n, _)| self.ident(self.name_of(n))).collect();
                let tuple = self.make_tuple(&ids);
                self.ast.add_attrs(tuple, Attrs::FOR_ARTIFACT);
                self.single_case_match(rhs, pat, tuple)
            }
        };
        let lazy = self.ast.modifiers(mods).is(flags::LAZY);
        match variables[..] {
            [] if !lazy => lowered,
            [(named, tpt)] => self.derived_val_def(span, named, tpt, Some(lowered), mods),
            _ => self.split_pat_def(pat, rhs, lowered, lowering, &variables, mods),
        }
    }

    /// How to take `rhs` apart for `pat`, and the variables of `pat` with their types.
    fn lowering(
        &mut self,
        pat: NodeId,
        rhs: NodeId,
        given_allowed: bool,
    ) -> (Lowering, Vec<(NodeId, NodeId)>) {
        if let Some((pats, _)) = self.tuple_pattern(pat) {
            let all: Vec<(NodeId, NodeId)> = pats
                .iter()
                .filter_map(|&p| match *self.ast.tree(p) {
                    Tree::Ident { .. } if self.is_var_pattern(p) => Some((p, self.type_tree())),
                    Tree::Typed { expr, tpt }
                        if matches!(self.ast.tree(expr), Tree::Ident { .. }) && self.is_var_pattern(expr) =>
                    {
                        Some((expr, tpt))
                    }
                    _ => None,
                })
                .collect();
            if all.len() == pats.len() {
                if self.forall_results(rhs, &|d, t| d.is_matching_tuple(t, pats.len())) {
                    return (Lowering::SimpleTuple, all);
                }
                if !self.may_be_unchecked(rhs) {
                    return (Lowering::MatchableTuple, all);
                }
            }
        }
        let vars = self.get_variables(pat, &mut |d, b| {
            if !given_allowed {
                d.error(b, GIVEN_IN_VAL);
            }
            given_allowed && !d.is_wildcard_name(b)
        });
        (Lowering::General, vars)
    }

    pub fn is_wildcard_name(&self, named: NodeId) -> bool {
        self.text(self.name_of(named)) == "_"
    }

    fn is_matching_tuple(&self, t: NodeId, arity: usize) -> bool {
        match *self.ast.tree(t) {
            Tree::Tuple { trees } => {
                let elems = self.ast.items(trees);
                elems.len() == arity
                    && !elems.iter().any(|&e| matches!(self.ast.tree(e), Tree::NamedArg { .. }))
            }
            _ => false,
        }
    }

    fn may_be_unchecked(&self, t: NodeId) -> bool {
        match *self.ast.tree(t) {
            Tree::Annotated { .. } => true,
            Tree::Select { name, .. } => self.text(name) == "runtimeChecked",
            _ => false,
        }
    }

    /// Whether `p` holds for all results of `t`, through `if`, `match`, blocks and parentheses.
    fn forall_results(&self, t: NodeId, p: &dyn Fn(&Self, NodeId) -> bool) -> bool {
        match *self.ast.tree(t) {
            Tree::If { thenp, elsep, .. } => {
                self.forall_results(thenp, p) && elsep.is_some_and(|e| self.forall_results(e, p))
            }
            Tree::Match { cases, .. } => self.ast.items(cases).iter().all(|&c| match *self.ast.tree(c) {
                Tree::CaseDef { body, .. } => self.forall_results(body, p),
                _ => false,
            }),
            Tree::Block { expr: Some(e), .. } | Tree::Parens { t: e } => self.forall_results(e, p),
            _ => p(self, t),
        }
    }

    /// The elements and type of `(ps)` or `(ps): T` (dotty's `TuplePattern`).
    pub fn tuple_pattern(&mut self, t: NodeId) -> Option<(Vec<NodeId>, NodeId)> {
        match *self.ast.tree(t) {
            Tree::Tuple { trees } => Some((self.ast.items(trees).to_vec(), self.type_tree())),
            Tree::Typed { expr, tpt } => match *self.ast.tree(expr) {
                Tree::Tuple { trees } => Some((self.ast.items(trees).to_vec(), tpt)),
                _ => None,
            },
            _ => None,
        }
    }
}
