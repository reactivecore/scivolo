//! Evidence parameters placed among a method's clauses (dotty's `addEvidenceParams` and
//! `fitEvidenceParams`).

use super::Desugar;
use super::context_bounds::WITNESS_NAMES;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::ModsId;
use crate::ast::{List, NodeId, TName, Tree};

impl Desugar<'_> {
    /// `meth` with evidence parameters `params` in the first clause that refers to them, the
    /// last clause if it is implicit, or a new last clause.
    pub fn add_evidence_params(&mut self, meth: NodeId, params: &[NodeId]) -> NodeId {
        if params.is_empty() {
            return meth;
        }
        let Tree::DefDef { name, paramss, tpt, rhs, mods } = *self.ast.tree(meth) else { return meth };
        let clauses = self.ast.clause_items(paramss).to_vec();
        let mut bound_names: Vec<TName> = params.iter().filter_map(|&p| self.def_name(p)).collect();
        for &l in &clauses {
            for &p in self.ast.items(l) {
                if let Tree::TypeDef { name, mods, .. } = *self.ast.tree(p)
                    && self.has_witness_names(mods)
                {
                    bound_names.push(TName { is_type: false, ..name });
                }
            }
        }
        let fitted = self.fit_evidence_params(params, &bound_names, &clauses);
        let paramss = self.ast.clauses(&fitted);
        self.ast.derived(meth, Tree::DefDef { name, paramss, tpt, rhs, mods })
    }

    fn has_witness_names(&self, mods: ModsId) -> bool {
        let annots = self.ast.modifiers(mods).annotations;
        self.ast.items(annots).iter().any(
            |&a| matches!(*self.ast.tree(a), Tree::TypedSplice { sym } if self.text(sym) == WITNESS_NAMES),
        )
    }

    /// `clauses` with `params` fitted in, both halves of dotty's result joined.
    fn fit_evidence_params(&mut self, params: &[NodeId], names: &[TName], clauses: &[List]) -> Vec<List> {
        let Some((&first, rest)) = clauses.split_first() else { return vec![self.list(params)] };
        if !self.is_type_clause(first) {
            let mparams = self.ast.items(first).to_vec();
            if mparams.iter().any(|&p| self.references_name(p, names)) {
                return [&[self.list(params)][..], clauses].concat();
            }
            if rest.is_empty()
                && let Some(&mparam) = mparams.first()
                && let Some(m) = self.ast.mods_of(mparam)
                && self.ast.modifiers(m).is_one_of(flags::GIVEN_OR_IMPLICIT)
            {
                let given = self.ast.modifiers(m).flags.and(flags::GIVEN_OR_IMPLICIT);
                let normalized = self.normalize_evidence(params, given);
                return vec![self.list(&[normalized, mparams].concat())];
            }
        }
        [&[first][..], &self.fit_evidence_params(params, names, rest)].concat()
    }

    /// `params` with the `given` or `implicit` of the clause they join.
    fn normalize_evidence(&mut self, params: &[NodeId], given: FlagSet) -> Vec<NodeId> {
        let head_given =
            |d: &Self, p: NodeId| d.ast.mods_of(p).is_some_and(|m| d.ast.modifiers(m).is(flags::GIVEN));
        if head_given(self, params[0]) == given.is(flags::GIVEN) {
            return params.to_vec();
        }
        params
            .iter()
            .map(|&p| {
                let Some(m) = self.ast.mods_of(p) else { return p };
                let m = self.ast.modifiers(m).clone();
                let flags = m.flags.and_not(flags::GIVEN_OR_IMPLICIT).or(given);
                self.ast.with_mods(p, Modifiers { flags, ..m })
            })
            .collect()
    }

    /// Whether the type of parameter `p` refers to a term in `names`.
    fn references_name(&self, p: NodeId, names: &[TName]) -> bool {
        let Tree::ValDef { tpt, .. } = *self.ast.tree(p) else { return false };
        self.ast.exists_subtree(
            tpt,
            &|ast, n| matches!(*ast.tree(n), Tree::Ident { name } if !name.is_type && names.contains(&name)),
        )
    }
}
