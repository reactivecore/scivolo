//! The types dotty's typer gives closures in capture-checked files, with the capture sets of their
//! parameters and results, where no expected type fixes the result.

use super::TreePickler;
use super::captured_exprs::Captured;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::capture_types::has_captures;
use tasty::{Binder, SymbolId, Type, TypeId};

/// A closure's method: its symbol, parameters, result type tree and right-hand side.
struct ClosureDef {
    method: SymbolId,
    params: Vec<TNodeId>,
    tpt: TNodeId,
    rhs: TNodeId,
}

impl TreePickler<'_> {
    /// Determines the type of block `block` that ends in a closure without an expected type:
    /// ours with the types of its method's parameters and result, the result told by its
    /// right-hand side (dotty's `typedFunctionValue`); `None` if one is not told. Records the
    /// method as one whose right-hand side tells its result.
    pub fn captured_closure(&mut self, block: TNodeId, depth: usize) -> Option<Captured> {
        let def = self.closure_def(block)?;
        let mut parts = Vec::new();
        for &p in &def.params {
            let T::ValDef { tpt, .. } = *self.tpd.tree(p) else { return None };
            parts.push(self.captured_tpt(tpt)?);
        }
        parts.push(self.closure_result(&def, depth)?);
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

    /// Determines the result type of closure block `block` passed for a function type of parameter
    /// types `formals` whose result is a type parameter of `binder` inferred: told by its
    /// right-hand side, if each parameter's type is written or a formal without capture sets or
    /// the binder's parameters, which records the parameter's inferred type tree as told.
    pub fn captured_closure_result(
        &mut self,
        block: TNodeId,
        (formals, binder): (&[TypeId], Binder),
        depth: usize,
    ) -> Option<Captured> {
        let def = self.closure_def(block)?;
        if def.params.len() != formals.len() {
            return None;
        }
        for (&p, &formal) in def.params.iter().zip(formals) {
            let T::ValDef { tpt, .. } = *self.tpd.tree(p) else { return None };
            if !matches!(self.tpd.tree(tpt), T::TypeTree { inferred: true }) {
                continue;
            }
            if has_captures(self.u, formal) || mentions(self.u, formal, binder) {
                return None;
            }
            self.inferred.capture_free.insert(tpt);
        }
        self.closure_result(&def, depth)
    }

    /// The result type of closure method `def`: a written one, else its right-hand side's.
    fn closure_result(&mut self, def: &ClosureDef, depth: usize) -> Option<Captured> {
        match self.tpd.tree(def.tpt) {
            T::TypeTree { inferred: true } => {
                self.inferred.free_closures.insert(def.method);
                self.captured_in(def.rhs, depth)
            }
            _ => self.captured_tpt(def.tpt),
        }
    }

    /// Determines the expression that gives the result type of the closure block `block` ends
    /// in, with the closure's method: its right-hand side if the result type is inferred.
    pub fn closure_result_expr(&self, block: TNodeId) -> Option<(TNodeId, SymbolId)> {
        let def = self.closure_def(block)?;
        let inferred = matches!(self.tpd.tree(def.tpt), T::TypeTree { inferred: true });
        Some((if inferred { def.rhs } else { def.tpt }, def.method))
    }

    /// Determines whether `t` is a block that ends in a closure.
    pub fn is_closure_block(&self, t: TNodeId) -> bool {
        self.closure_def(t).is_some()
    }

    /// The method of one parameter clause of the closure block `block` ends in, inside blocks
    /// without statements (`f { x => .. }`).
    fn closure_def(&self, mut block: TNodeId) -> Option<ClosureDef> {
        while let T::Block { stats, expr } = *self.tpd.tree(block)
            && stats.is_empty()
        {
            block = expr;
        }
        let T::Block { stats, expr } = *self.tpd.tree(block) else { return None };
        let T::Closure { meth, tpt: None, .. } = *self.tpd.tree(expr) else { return None };
        let method = self.tpd.sym(meth)?;
        let def = self.tpd.items(stats).iter().copied().find(|&s| self.tpd.sym(s) == Some(method))?;
        let T::DefDef { paramss, tpt, rhs, .. } = self.tpd.tree(def).clone() else { return None };
        let [clause] = self.tpd.clause_items(paramss) else { return None };
        Some(ClosureDef { method, params: self.tpd.items(*clause).to_vec(), tpt, rhs: rhs? })
    }
}

/// Whether `t` refers to a parameter of `binder`.
fn mentions(u: &mut tasty::Universe, t: TypeId, binder: Binder) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= match *u.ty(part) {
            Type::Param(b, _) => b == binder,
            Type::Local(b, _) => b.0 == binder.0,
            _ => false,
        };
        None
    });
    found
}
