//! Default arguments as getter methods `f$default$N` (dotty's `addDefaultGetters`).

use super::Desugar;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::Clauses;
use crate::ast::{List, NodeId, Tree};

impl Desugar<'_> {
    /// Method `meth` (dotty's `defDef`): evidence for its context bounds, and a getter for each
    /// default argument.
    pub fn def_def(&mut self, meth: NodeId) -> NodeId {
        let meth = self.elim_context_bounds(meth);
        self.add_default_getters(meth)
    }

    fn add_default_getters(&mut self, meth: NodeId) -> NodeId {
        let Tree::DefDef { name, paramss, tpt, rhs, mods } = *self.ast.tree(meth) else { return meth };
        let clauses = self.ast.clause_items(paramss).to_vec();
        let vparams: Vec<NodeId> = clauses
            .iter()
            .filter(|&&l| !self.is_type_clause(l))
            .flat_map(|&l| self.ast.items(l).to_vec())
            .collect();
        let mut getters = Vec::new();
        for (n, &vparam) in vparams.iter().enumerate().rev() {
            let Tree::ValDef { rhs: Some(default), mods: pmods, .. } = *self.ast.tree(vparam) else {
                continue;
            };
            if matches!(self.ast.tree(default), Tree::Empty) {
                continue;
            }
            let getter_paramss = self.getter_paramss(&clauses, n);
            let base = if self.text(name) == "<init>" { "$lessinit$greater" } else { self.text(name) };
            let getter_name = self.term(&format!("{base}$default${}", n + 1));
            let m = self.ast.modifiers(mods);
            let access = m.flags.and(flags::ACCESS_FLAGS.or(flags::SYNTHETIC));
            let inline = self.ast.modifiers(pmods).flags.and(flags::INLINE);
            let getter_mods = self.ast.mods(Modifiers {
                flags: access.or(inline),
                private_within: m.private_within,
                ..Modifiers::default()
            });
            let getter_tpt = self.type_tree();
            let getter = self.ast.node(Tree::DefDef {
                name: getter_name,
                paramss: getter_paramss,
                tpt: getter_tpt,
                rhs: Some(default),
                mods: getter_mods,
            });
            let span = self.ast.span(default);
            getters.push(self.ast.respan(getter, span));
        }
        if getters.is_empty() {
            return meth;
        }
        getters.reverse();
        let no_rhs = self.paramss_no_rhs(&clauses);
        let paramss = self.ast.clauses(&no_rhs);
        let meth = self.ast.derived(meth, Tree::DefDef { name, paramss, tpt, rhs, mods });
        let trees = self.list(&[&[meth][..], &getters].concat());
        self.ast.node(Tree::Thicket { trees })
    }

    /// `clauses` with the defaults of value parameters dropped and marked `HasDefault`.
    fn paramss_no_rhs(&mut self, clauses: &[List]) -> Vec<List> {
        clauses
            .iter()
            .map(|&l| {
                if self.is_type_clause(l) {
                    return l;
                }
                let params = self.ast.items(l).to_vec();
                let mapped: Vec<NodeId> = params
                    .iter()
                    .map(|&p| {
                        let Tree::ValDef { name, tpt, rhs: Some(r), mods } = *self.ast.tree(p) else {
                            return p;
                        };
                        if matches!(self.ast.tree(r), Tree::Empty) {
                            return p;
                        }
                        let m = self.ast.modifiers(mods).clone().or(flags::HAS_DEFAULT);
                        let p = self.ast.derived(p, Tree::ValDef { name, tpt, rhs: None, mods });
                        self.ast.with_mods(p, m)
                    })
                    .collect();
                if mapped == params { l } else { self.list(&mapped) }
            })
            .collect()
    }

    /// The parameter clauses of the getter of the default of value parameter `n`: the type
    /// parameters and the whole value clauses before it.
    fn getter_paramss(&mut self, clauses: &[List], n: usize) -> Clauses {
        let no_rhs = self.paramss_no_rhs(clauses);
        let mut left = n;
        let mut taken = Vec::new();
        for l in no_rhs {
            let params = self.ast.items(l).to_vec();
            let params = if self.is_type_clause(l) {
                params.iter().map(|&t| self.type_param_of_getter(t)).collect::<Vec<_>>()
            } else if params.len() <= left {
                left -= params.len();
                params.iter().map(|&v| self.value_param_of_getter(v)).collect()
            } else {
                break;
            };
            taken.push(self.list(&params));
        }
        self.ast.clauses(&taken)
    }

    fn type_param_of_getter(&mut self, tparam: NodeId) -> NodeId {
        let Some(mods) = self.ast.mods_of(tparam) else { return tparam };
        let m = self.ast.modifiers(mods).clone();
        let tparam = self.ast.with_mods(tparam, Modifiers { flags: flags::PARAM, ..m });
        let Tree::TypeDef { name, rhs, mods } = *self.ast.tree(tparam) else { return tparam };
        let rhs = self.drop_context_bounds(rhs);
        self.ast.derived(tparam, Tree::TypeDef { name, rhs, mods })
    }

    fn drop_context_bounds(&mut self, rhs: NodeId) -> NodeId {
        match *self.ast.tree(rhs) {
            Tree::ContextBounds { bounds, .. } => bounds,
            Tree::LambdaTypeTree { tparams, body } => {
                let body = self.drop_context_bounds(body);
                self.ast.derived(rhs, Tree::LambdaTypeTree { tparams, body })
            }
            _ => rhs,
        }
    }

    fn value_param_of_getter(&mut self, vparam: NodeId) -> NodeId {
        let Tree::ValDef { name, tpt, rhs, mods } = *self.ast.tree(vparam) else { return vparam };
        let m = self.ast.modifiers(mods).clone();
        let kept = flags::GIVEN_OR_IMPLICIT.or(flags::ERASED).or(flags::TRACKED);
        let m = Modifiers { flags: m.flags.and(kept).or(flags::PARAM), ..m };
        let fresh_mods = self.ast.mods(m);
        let param = self.ast.node(Tree::ValDef { name, tpt, rhs, mods: fresh_mods });
        let span = self.ast.span(vparam);
        let param = self.ast.respan(param, span);
        self.ast.add_attrs(param, self.ast.attrs(vparam));
        param
    }
}
