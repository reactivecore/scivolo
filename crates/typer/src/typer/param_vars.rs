//! The type variables of an application instantiated before its implicit arguments are searched
//! (dotty's `tvarsInParams` and `instantiateSelected`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Instantiates the type variables that type applications in `tree`'s prefixes bind and
    /// the parameters of its applications mention, where they occur in `params`: minimized if
    /// they have a lower bound, else maximized if they have an upper bound.
    pub fn instantiate_param_vars(&mut self, tree: TNodeId, params: &[TypeId]) -> Result<(), &'static str> {
        let mut bound = Vec::new();
        self.bound_vars(tree, &mut bound);
        let mut selected = Vec::new();
        self.occurring_vars(tree, bound, &mut selected);
        for &p in params {
            for v in self.open_vars(p) {
                if !selected.contains(&v) || self.constraint.instance(v).is_some() {
                    continue;
                }
                let (lo, hi) = self.constraint.bounds(v);
                let (lo, hi) = (self.zonk(lo), self.zonk(hi));
                if lo != NOTHING {
                    self.instantiate_var(v, true)?;
                } else if hi != ANY {
                    self.instantiate_var(v, false)?;
                }
            }
        }
        Ok(())
    }

    /// The uninstantiated type variables in `t`.
    fn open_vars(&mut self, t: TypeId) -> Vec<VarId> {
        let t = self.zonk(t);
        let mut vars = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part)
                && !vars.contains(&v)
            {
                vars.push(v);
            }
            None
        });
        vars
    }

    fn bound_vars(&mut self, tree: TNodeId, acc: &mut Vec<VarId>) {
        match *self.tpd.tree(tree) {
            T::Apply { fun, args } => {
                for a in self.tpd.items(args).to_vec() {
                    self.bound_vars(a, acc);
                }
                self.bound_vars(fun, acc);
            }
            T::TypeApply { fun, args } => {
                for a in self.tpd.items(args).to_vec() {
                    let tpe = self.zonk(self.tpd.tpe(a));
                    if matches!(self.tpd.tree(a), T::TypeTree { inferred: true })
                        && let Type::Var(v) = *self.u.types().get(tpe)
                    {
                        acc.push(v);
                    }
                }
                self.bound_vars(fun, acc);
            }
            T::Select { qualifier, .. } => self.bound_vars(qualifier, acc),
            T::Block { expr, .. } => self.bound_vars(expr, acc),
            _ => {}
        }
    }

    /// The variables of `to_test` that occur in the parameter types of an application in
    /// `tree`'s prefixes, added to `acc`.
    fn occurring_vars(&mut self, tree: TNodeId, to_test: Vec<VarId>, acc: &mut Vec<VarId>) {
        if to_test.is_empty() {
            return;
        }
        match *self.tpd.tree(tree) {
            T::Apply { fun, args } => {
                let mut in_args = Vec::new();
                for a in self.tpd.items(args).to_vec() {
                    self.occurring_vars(a, to_test.clone(), &mut in_args);
                }
                let mut rest: Vec<VarId> = to_test.into_iter().filter(|v| !in_args.contains(v)).collect();
                let fun_tpe = self.tpd.tpe(fun);
                let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
                if let Type::Method { params, .. } = self.u.types().get(info).clone() {
                    let mentioned: Vec<VarId> = params.iter().flat_map(|&(_, p)| self.open_vars(p)).collect();
                    acc.extend(rest.iter().filter(|v| mentioned.contains(v)));
                    rest.retain(|v| !mentioned.contains(v));
                }
                acc.extend(in_args);
                self.occurring_vars(fun, rest, acc);
            }
            T::TypeApply { fun, .. } => self.occurring_vars(fun, to_test, acc),
            T::Select { qualifier, .. } => self.occurring_vars(qualifier, to_test, acc),
            T::Block { expr, .. } => self.occurring_vars(expr, to_test, acc),
            _ => {}
        }
    }
}
