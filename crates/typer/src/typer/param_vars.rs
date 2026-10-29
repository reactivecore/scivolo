// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! The type variables of an application instantiated before its implicit arguments are searched
//! (dotty's `tvarsInParams` and `instantiateSelected`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Instantiates the type variables other than locked ones that type applications in `tree`'s
    /// prefixes bind and the parameters of its applications mention, where they occur in
    /// `params`: minimized if they have a lower bound, else maximized if they have an upper bound.
    pub fn instantiate_param_vars(&mut self, tree: TNodeId, params: &[TypeId]) -> Result<(), &'static str> {
        let mut bound = Vec::new();
        self.bound_vars(tree, &mut bound);
        let declared: Vec<(VarId, TypeId)> = bound.clone();
        let mut selected = Vec::new();
        self.occurring_vars(tree, bound.into_iter().map(|(v, _)| v).collect(), &mut selected);
        for &p in params {
            for v in self.open_vars(p) {
                if !selected.contains(&v) || self.constraint.instance(v).is_some() {
                    continue;
                }
                let (lo, hi) = self.constraint.bounds(v);
                let (lo, hi) = (self.zonk(lo), self.zonk(hi));
                let declared_lo = declared.iter().find(|&&(w, _)| w == v).map_or(NOTHING, |&(_, l)| l);
                if matches!(self.u.types().get(lo), Type::And(..)) {
                    return Err("type variable minimized to an intersection before an implicit search");
                } else if lo != NOTHING && declared_lo != NOTHING && hi != ANY {
                    return Err("type variable with a declared lower bound constrained from above");
                } else if lo != NOTHING {
                    self.instantiate_var(v, true)?;
                } else if !self.is_any(hi) && !matches!(self.u.types().get(hi), Type::Var(_)) {
                    self.instantiate_var(v, false)?;
                }
            }
        }
        Ok(())
    }

    /// Whether `t` is `Any` or `AnyKind`.
    fn is_any(&mut self, t: TypeId) -> bool {
        let builtins = self.u.builtins();
        t == ANY || self.u.class_symbol(t).is_some_and(|c| c == builtins.any || c == builtins.any_kind)
    }

    /// The uninstantiated type variables in `t`.
    pub fn open_vars(&mut self, t: TypeId) -> Vec<VarId> {
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

    /// The type variables inferred type arguments in `tree`'s prefixes bind, with the declared
    /// lower bounds of their parameters.
    fn bound_vars(&mut self, tree: TNodeId, acc: &mut Vec<(VarId, TypeId)>) {
        match *self.tpd.tree(tree) {
            T::Apply { fun, args } => {
                for a in self.tpd.items(args).to_vec() {
                    self.bound_vars(a, acc);
                }
                self.bound_vars(fun, acc);
            }
            T::TypeApply { fun, args } => {
                let fun_tpe = self.tpd.tpe(fun);
                let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
                let bounds = match self.u.types().get(info) {
                    Type::Poly { params, .. } => params.iter().map(|&(_, b)| b).collect(),
                    _ => Vec::new(),
                };
                for (i, a) in self.tpd.items(args).to_vec().into_iter().enumerate() {
                    let tpe = self.zonk(self.tpd.tpe(a));
                    if matches!(self.tpd.tree(a), T::TypeTree { inferred: true })
                        && let Type::Var(v) = *self.u.types().get(tpe)
                        && v.0 as usize >= self.locked_vars
                    {
                        let lo = match bounds.get(i).map(|&b| self.u.types().get(b).clone()) {
                            Some(Type::Bounds(lo, _))
                                if self.u.class_symbol(lo) != Some(self.u.builtins().nothing) =>
                            {
                                lo
                            }
                            _ => NOTHING,
                        };
                        acc.push((v, lo));
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
                if let Some(params) = self.u.method(info).map(|m| m.param_types()) {
                    let mentioned: Vec<VarId> = params.iter().flat_map(|&p| self.open_vars(p)).collect();
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
