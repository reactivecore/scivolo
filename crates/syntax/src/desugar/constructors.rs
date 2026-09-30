//! Constructors of classes and references to them: secondary constructors with the class's
//! type parameters and evidence, methods split from their default getters, the applied class.

use super::Desugar;
use super::class_params::Keep;
use crate::ast::flags;
use crate::ast::span::Span;
use crate::ast::{List, NodeId, TName, Tree};
use crate::names::to_utf16;

impl Desugar<'_> {
    /// Secondary constructor `ddef` with the class's type parameters and evidence, and its
    /// default getters.
    pub fn expand_constructor(
        &mut self,
        ddef: NodeId,
        tparams: &[NodeId],
        constr1: NodeId,
    ) -> (NodeId, Vec<NodeId>) {
        let own: Vec<Vec<NodeId>> =
            self.clauses_of(ddef).iter().map(|&l| self.ast.items(l).to_vec()).collect();
        let joined = self.join_params(tparams, &own);
        let ddef = self.with_clauses(ddef, &joined);
        let evidence: Vec<NodeId> = self
            .evidence_params(constr1)
            .into_iter()
            .map(|p| self.meth_value_param(p, Keep::None, false))
            .collect();
        let ddef = self.add_evidence_params(ddef, &evidence);
        let expanded = self.def_def(ddef, false);
        self.decompose(expanded)
    }

    /// A method and its default getters (dotty's `decompose`).
    pub fn decompose(&self, t: NodeId) -> (NodeId, Vec<NodeId>) {
        match *self.ast.tree(t) {
            Tree::Thicket { trees } => {
                let items = self.ast.items(trees);
                (items[0], items[1..].to_vec())
            }
            _ => (t, Vec::new()),
        }
    }

    pub fn clauses_of(&self, ddef: NodeId) -> Vec<List> {
        match *self.ast.tree(ddef) {
            Tree::DefDef { paramss, .. } => self.ast.clause_items(paramss).to_vec(),
            _ => Vec::new(),
        }
    }

    /// The span of the name of definition `def` (dotty's `nameSpan`).
    pub fn name_span(&self, def: NodeId, name: TName) -> Span {
        let span = self.ast.span(def);
        let synthetic = self.ast.mods_of(def).is_some_and(|m| self.ast.modifiers(m).is(flags::SYNTHETIC));
        if !span.exists() || synthetic || span.is_synthetic() {
            return Span::at(span.point());
        }
        let units = to_utf16(self.text(name)).len();
        Span::new(span.point(), self.source.advance_utf16(span.point(), units), span.point())
    }

    /// `tycon` applied to the class's type parameters, at the start of the class (dotty's
    /// `appliedRef`).
    pub fn applied_ref(&mut self, cdef: NodeId, tparams: &[NodeId], tycon: NodeId) -> NodeId {
        let args: Vec<NodeId> = tparams
            .iter()
            .filter_map(|&t| match *self.ast.tree(t) {
                Tree::TypeDef { name, .. } => Some(self.ident(name)),
                _ => None,
            })
            .collect();
        let t = if args.is_empty() {
            tycon
        } else {
            let args = self.list(&args);
            self.ast.node(Tree::AppliedTypeTree { tpt: tycon, args })
        };
        let start = self.ast.span(cdef).start_pos();
        self.ast.respan(t, start)
    }

    /// Method `ddef` with parameter clauses `clauses`.
    pub fn with_clauses(&mut self, ddef: NodeId, clauses: &[List]) -> NodeId {
        let Tree::DefDef { name, tpt, rhs, mods, .. } = *self.ast.tree(ddef) else { return ddef };
        let paramss = self.ast.clauses(clauses);
        self.ast.derived(ddef, Tree::DefDef { name, paramss, tpt, rhs, mods })
    }
}
