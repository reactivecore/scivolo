//! A pattern definition of several variables: a temporary for the value and a definition per
//! variable selecting from it.

use super::Desugar;
use super::exprs::MAX_TUPLE_ARITY;
use super::pat_def::Lowering;
use crate::ast::constant::Constant;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::ModsId;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// The definitions of `variables` from `lowered`, the value of `pat = rhs`.
    pub fn split_pat_def(
        &mut self,
        pat: NodeId,
        rhs: NodeId,
        lowered: NodeId,
        lowering: Lowering,
        variables: &[(NodeId, NodeId)],
        mods: ModsId,
    ) -> NodeId {
        let m = self.ast.modifiers(mods).clone();
        let lazy = m.is(flags::LAZY);
        let all_named = variables.iter().all(|&(n, _)| !self.is_wildcard_name(n));
        let rhs_elems = match *self.ast.tree(rhs) {
            Tree::Tuple { trees }
                if lowering == Lowering::SimpleTuple
                    && trees.len() == variables.len()
                    && !matches!(self.ast.tree(pat), Tree::Typed { .. })
                    && !lazy
                    && all_named =>
            {
                Some(self.ast.items(trees).to_vec())
            }
            _ => None,
        };
        let mut defs = Vec::new();
        let tmp = if rhs_elems.is_none() {
            let tmp = TName { name: self.fresh.unique(self.names), is_type: false };
            let local = if self.owner.is_class() { flags::PRIVATE_LOCAL } else { flags::EMPTY };
            let flags = m.flags.and(flags::LAZY).or(flags::SYNTHETIC).or(local);
            let tmp_mods = self.ast.mods(Modifiers { flags, ..m.clone() });
            let tpt = match self.tuple_pattern(pat) {
                Some((_, tpt)) if lowering != Lowering::General => tpt,
                _ => self.type_tree(),
            };
            let vdef = self.ast.node(Tree::ValDef { name: tmp, tpt, rhs: Some(lowered), mods: tmp_mods });
            let span = self.ast.span(pat).union(self.ast.span(rhs));
            defs.push(self.ast.respan(vdef, span));
            Some(tmp)
        } else {
            None
        };
        for (idx, &(named, tpt)) in variables.iter().enumerate() {
            if self.is_wildcard_name(named) {
                continue;
            }
            let selector = match (&rhs_elems, tmp) {
                (Some(elems), _) => elems[idx],
                (None, Some(tmp)) => self.tmp_selector(tmp, idx, variables.len()),
                (None, None) => unreachable!("a temporary"),
            };
            let (name, span) = (self.name_of(named), self.ast.span(named));
            let rhs = Some(selector);
            let def = if lazy {
                let def_mods = self.ast.mods(Modifiers { flags: m.flags.and_not(flags::LAZY), ..m.clone() });
                let paramss = self.ast.clauses(&[]);
                self.ast.node(Tree::DefDef { name, paramss, tpt, rhs, mods: def_mods })
            } else {
                self.ast.node(Tree::ValDef { name, tpt, rhs, mods })
            };
            let def = self.ast.respan(def, span);
            self.ast.add_attrs(def, Attrs::PATTERN_VAR);
            defs.push(if lazy { def } else { self.val_def(def) });
        }
        self.flat_tree(&defs)
    }

    /// `tmp._N`, or `tmp.apply(idx)` beyond the largest tuple.
    fn tmp_selector(&mut self, tmp: TName, idx: usize, count: usize) -> NodeId {
        let tmp = self.ident(tmp);
        if count <= MAX_TUPLE_ARITY {
            let name = self.term(&format!("_{}", idx + 1));
            self.select(tmp, name)
        } else {
            let apply = self.term("apply");
            let fun = self.select(tmp, apply);
            let index = self.literal(Constant::Int(idx as i32));
            self.apply(fun, &[index])
        }
    }
}
