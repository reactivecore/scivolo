//! The function types dotty's typer gives closures without an expected type in capture-checked
//! files, with the capture sets of their parameters and results.

use super::TreePickler;
use super::captured_exprs::Captured;
use crate::tpd::{TNodeId, Tree as T};

impl TreePickler<'_> {
    /// Determines the type of block `block` that ends in closure `closure` of a method of one
    /// parameter clause defined in it: ours with the types of the method's parameters and result,
    /// the result inferred from its right-hand side (dotty's `typedFunctionValue` without an
    /// expected type); `None` if one is not told. Records the method as one whose right-hand side
    /// tells its result.
    pub fn captured_closure(&mut self, block: TNodeId, closure: TNodeId, depth: usize) -> Option<Captured> {
        let (T::Block { stats, .. }, T::Closure { meth, .. }) =
            (self.tpd.tree(block), self.tpd.tree(closure))
        else {
            return None;
        };
        let method = self.tpd.sym(*meth)?;
        let def = self.tpd.items(*stats).iter().copied().find(|&s| self.tpd.sym(s) == Some(method))?;
        let T::DefDef { paramss, tpt, rhs, .. } = self.tpd.tree(def).clone() else { return None };
        let [clause] = self.tpd.clause_items(paramss) else { return None };
        let params = self.tpd.items(*clause).to_vec();
        let mut parts = Vec::new();
        for p in params {
            let T::ValDef { tpt, .. } = *self.tpd.tree(p) else { return None };
            parts.push(self.captured_tpt(tpt)?);
        }
        let result = match self.tpd.tree(tpt) {
            T::TypeTree { inferred: true } => {
                self.inferred.free_closures.insert(method);
                self.captured_in(rhs?, depth)?
            }
            _ => self.captured_tpt(tpt)?,
        };
        parts.push(result);
        if parts.iter().all(|&p| p == Captured::Ours) {
            return Some(Captured::Ours);
        }
        let (tycon, args) = self.u.applied(self.tpd.get(block).tpe?).map(|(c, a)| (c, a.to_vec()))?;
        if args.len() != parts.len() {
            return None;
        }
        let args: Vec<_> = (args.iter().zip(&parts))
            .map(|(&a, &p)| match p {
                Captured::With(t) => t,
                Captured::Ours => a,
            })
            .collect();
        Some(Captured::With(self.u.intern_applied(tycon, &args)))
    }
}
