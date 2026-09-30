//! Context bounds as evidence parameters (dotty's `desugarContextBounds` and
//! `elimContextBounds`).

use super::Desugar;
use crate::ast::flags::{self, FlagSet};
use crate::ast::{Attrs, List, NodeId, TName, Tree};
use crate::config::{Features, SourceVersion};
use crate::names::to_utf16;

pub const WITNESS_NAMES: &str = "scala.annotation.internal.WitnessNames";

impl Desugar<'_> {
    /// Type definition `tdef` without context bounds; their evidence goes to `evidence`, named
    /// `evidence$N` from `counter`, or as invented given names without one.
    pub fn desugar_context_bounds(
        &mut self,
        tdef: NodeId,
        evidence: &mut Vec<NodeId>,
        evidence_flags: FlagSet,
        mut counter: Option<&mut u32>,
        all_paramss: &[List],
    ) -> NodeId {
        let Tree::TypeDef { name, rhs, mods } = *self.ast.tree(tdef) else { return tdef };
        let mut names = Vec::new();
        let rhs = self.context_bounds_rhs(rhs, evidence, evidence_flags, &mut counter, &mut names);
        let tdef1 = self.ast.derived(tdef, Tree::TypeDef { name, rhs, mods });
        let as_term = TName { is_type: false, ..name };
        let clash = evidence.iter().any(|&e| self.def_name(e) == Some(as_term))
            || all_paramss
                .iter()
                .any(|&l| self.ast.items(l).iter().any(|&p| self.def_name(p) == Some(as_term)));
        if !self.cfg.features.contains(Features::MODULARITY) || names.is_empty() || clash {
            return tdef1;
        }
        let annot = self.sym_ref(WITNESS_NAMES, true);
        let annot = self.ast.respan(annot, self.ast.span(tdef));
        let Some(mods) = self.ast.mods_of(tdef1) else { return tdef1 };
        let mut m = self.ast.modifiers(mods).clone();
        let annots = [self.ast.items(m.annotations), &[annot]].concat();
        m.annotations = self.list(&annots);
        self.ast.with_mods(tdef1, m)
    }

    fn context_bounds_rhs(
        &mut self,
        rhs: NodeId,
        evidence: &mut Vec<NodeId>,
        evidence_flags: FlagSet,
        counter: &mut Option<&mut u32>,
        names: &mut Vec<TName>,
    ) -> NodeId {
        match *self.ast.tree(rhs) {
            Tree::ContextBounds { bounds, cx_bounds } => {
                for bound in self.ast.items(cx_bounds).to_vec() {
                    let span = self.ast.span(bound);
                    let (name, point) = match *self.ast.tree(bound) {
                        Tree::ContextBoundTypeTree { own_name, .. } if !self.text(own_name).is_empty() => {
                            let units = to_utf16(self.text(own_name)).len();
                            (own_name, self.source.retreat_utf16(span.end(), units))
                        }
                        _ => {
                            let name = match counter {
                                Some(n) => {
                                    **n += 1;
                                    self.term(&format!("evidence${n}"))
                                }
                                None => self.invent_given_name(bound),
                            };
                            (name, span.point())
                        }
                    };
                    names.push(name);
                    let mods = self.mods(evidence_flags);
                    let param = self.ast.node(Tree::ValDef { name, tpt: bound, rhs: None, mods });
                    let param = self.ast.respan(param, span.with_point(point));
                    self.ast.add_attrs(param, Attrs::EVIDENCE);
                    evidence.push(param);
                }
                bounds
            }
            Tree::LambdaTypeTree { tparams, body } => {
                let body = self.context_bounds_rhs(body, evidence, evidence_flags, counter, names);
                self.ast.derived(rhs, Tree::LambdaTypeTree { tparams, body })
            }
            _ => rhs,
        }
    }

    pub fn def_name(&self, t: NodeId) -> Option<TName> {
        match *self.ast.tree(t) {
            Tree::ValDef { name, .. } | Tree::TypeDef { name, .. } | Tree::DefDef { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Method `meth` with its context bounds replaced by evidence parameters.
    pub fn elim_context_bounds(&mut self, meth: NodeId, primary: bool) -> NodeId {
        let Tree::DefDef { name, paramss, tpt, rhs, mods } = *self.ast.tree(meth) else { return meth };
        let clauses = self.ast.clause_items(paramss).to_vec();
        let iflag = clauses
            .last()
            .and_then(|&l| self.ast.items(l).first().copied())
            .and_then(|p| self.ast.mods_of(p))
            .map(|m| self.ast.modifiers(m).flags.and(flags::GIVEN_OR_IMPLICIT))
            .filter(|f| !f.is_empty())
            .unwrap_or(if self.cfg.source_version.is_at_least(SourceVersion::V3_6) {
                flags::GIVEN
            } else {
                flags::IMPLICIT
            });
        let evidence_flags = iflag.or(if primary { flags::LOCAL_PARAM_ACCESSOR } else { flags::PARAM });
        let mut evidence = Vec::new();
        let mut seen = 0;
        let mapped: Vec<List> = clauses
            .iter()
            .map(|&l| {
                if !self.is_type_clause(l) {
                    return l;
                }
                let tparams = self.ast.items(l).to_vec();
                let new: Vec<NodeId> = tparams
                    .iter()
                    .map(|&t| {
                        self.desugar_context_bounds(
                            t,
                            &mut evidence,
                            evidence_flags,
                            Some(&mut seen),
                            &clauses,
                        )
                    })
                    .collect();
                if new == tparams { l } else { self.list(&new) }
            })
            .collect();
        if let Some(r) = rhs
            && let Tree::MacroTree { expr } = *self.ast.tree(r)
        {
            let m = self.ast.modifiers(mods).clone().or(flags::MACRO.or(flags::ERASED));
            let meth = self.ast.derived(meth, Tree::DefDef { name, paramss, tpt, rhs: Some(expr), mods });
            return self.ast.with_mods(meth, m);
        }
        let name = self.normalize_name(name, tpt);
        let paramss = if mapped == clauses { paramss } else { self.ast.clauses(&mapped) };
        let meth = self.ast.derived(meth, Tree::DefDef { name, paramss, tpt, rhs, mods });
        self.add_evidence_params(meth, &evidence)
    }

    pub fn is_type_clause(&self, l: List) -> bool {
        self.ast.items(l).first().is_some_and(|&p| matches!(self.ast.tree(p), Tree::TypeDef { .. }))
    }
}
