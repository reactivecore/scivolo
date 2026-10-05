//! The parameters of a class: its constructor's, and the copies dotty derives from them for
//! accessors and synthetic methods (dotty's `toMethParam`, `derivedTypeParam`,
//! `derivedTermParam`).

use super::Desugar;
use super::context_bounds::WITNESS_NAMES;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::DerivedKind;
use crate::ast::{Attrs, List, NodeId, Tree};

/// Which annotations a derived parameter keeps.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    None,
    All,
    WitnessOnly,
}

impl Desugar<'_> {
    fn kept_annotations(&mut self, annotations: List, keep: Keep) -> List {
        match keep {
            Keep::All => annotations,
            Keep::None => self.list(&[]),
            Keep::WitnessOnly => {
                let kept: Vec<NodeId> = self
                    .ast
                    .items(annotations)
                    .iter()
                    .copied()
                    .filter(|&a| matches!(*self.ast.tree(a), Tree::TypedSplice { sym } if self.text(sym) == WITNESS_NAMES))
                    .collect();
                if kept.len() == annotations.len() { annotations } else { self.list(&kept) }
            }
        }
    }

    /// Type parameter `tparam` as a method's (dotty's `toMethParam`).
    pub fn meth_type_param(&mut self, tparam: NodeId, keep: Keep) -> NodeId {
        let Some(mods) = self.ast.mods_of(tparam) else { return tparam };
        let m = self.ast.modifiers(mods).clone();
        let annotations = self.kept_annotations(m.annotations, keep);
        self.ast.with_mods(tparam, Modifiers { flags: flags::PARAM, annotations, ..m })
    }

    /// Value parameter `vparam` as a new method parameter (dotty's `toMethParam`).
    pub fn meth_value_param(&mut self, vparam: NodeId, keep: Keep, keep_default: bool) -> NodeId {
        let Tree::ValDef { name, tpt, rhs, mods } = *self.ast.tree(vparam) else { return vparam };
        let m = self.ast.modifiers(mods).clone();
        let annotations = self.kept_annotations(m.annotations, keep);
        let default = if keep_default { flags::HAS_DEFAULT } else { flags::EMPTY };
        let kept = flags::GIVEN_OR_IMPLICIT.or(flags::ERASED).or(default).or(flags::TRACKED);
        let mods = self.ast.mods(Modifiers { flags: m.flags.and(kept).or(flags::PARAM), annotations, ..m });
        let param = self.ast.node(Tree::ValDef { name, tpt, rhs, mods });
        let span = self.ast.span(vparam);
        let param = self.ast.respan(param, span);
        self.ast.add_attrs(param, self.ast.attrs(vparam));
        param
    }

    /// `tdef` with a type derived from it as its rhs (dotty's `derivedTypeParam`).
    pub fn derived_type_param(&mut self, tdef: NodeId) -> NodeId {
        let Tree::TypeDef { name, rhs, mods } = *self.ast.tree(tdef) else { return tdef };
        let derived =
            self.ast.node(Tree::DerivedTypeTree { kind: DerivedKind::FromParam, watched: Some(name) });
        let derived = self.ast.respan(derived, self.ast.span(rhs));
        self.ast.derived(tdef, Tree::TypeDef { name, rhs: derived, mods })
    }

    /// `vdef` with a type derived from it (dotty's `derivedTermParam`).
    pub fn derived_term_param(&mut self, vdef: NodeId) -> NodeId {
        let Tree::ValDef { name, tpt, rhs, mods } = *self.ast.tree(vdef) else { return vdef };
        let derived =
            self.ast.node(Tree::DerivedTypeTree { kind: DerivedKind::FromParam, watched: Some(name) });
        let derived = self.ast.respan(derived, self.ast.span(tpt));
        self.ast.derived(vdef, Tree::ValDef { name, tpt: derived, rhs, mods })
    }

    /// Definition `id` with annotations `annotations`.
    pub fn with_annotations(&mut self, id: NodeId, annotations: List) -> NodeId {
        let Some(mods) = self.ast.mods_of(id) else { return id };
        let m = self.ast.modifiers(mods).clone();
        self.ast.with_mods(id, Modifiers { annotations, ..m })
    }

    /// Copies of type parameters `tparams` and of their bounds trees, for another method.
    pub fn fresh_type_params(&mut self, tparams: &[NodeId]) -> Vec<NodeId> {
        tparams
            .iter()
            .map(|&t| {
                let copy = self.ast.copy_node(t);
                if let Tree::TypeDef { rhs, .. } = *self.ast.tree(copy) {
                    let fresh = self.ast.copy_node(rhs);
                    if let Tree::TypeDef { rhs, .. } = &mut self.ast.get_mut(copy).tree {
                        *rhs = fresh;
                    }
                }
                copy
            })
            .collect()
    }

    /// `tparams` as a leading clause of `paramss` unless empty (dotty's `joinParams`).
    pub fn join_params(&mut self, tparams: &[NodeId], paramss: &[Vec<NodeId>]) -> Vec<List> {
        let mut clauses = Vec::with_capacity(paramss.len() + 1);
        if !tparams.is_empty() {
            clauses.push(self.list(tparams));
        }
        for params in paramss {
            clauses.push(self.list(params));
        }
        clauses
    }

    /// The evidence parameters of constructor `constr` (dotty's `evidenceParams`).
    pub fn evidence_params(&self, constr: NodeId) -> Vec<NodeId> {
        let Tree::DefDef { paramss, .. } = *self.ast.tree(constr) else { return Vec::new() };
        let mut out = Vec::new();
        for &l in self.ast.clause_items(paramss) {
            let params = self.ast.items(l);
            let implicit = params.first().is_some_and(|&p| {
                matches!(self.ast.tree(p), Tree::ValDef { .. })
                    && self
                        .ast
                        .mods_of(p)
                        .is_some_and(|m| self.ast.modifiers(m).is_one_of(flags::GIVEN_OR_IMPLICIT))
            });
            if implicit {
                out.extend(
                    params.iter().copied().take_while(|&p| self.ast.attrs(p).contains(Attrs::EVIDENCE)),
                );
            }
        }
        out
    }
}
