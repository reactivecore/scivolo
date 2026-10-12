//! The types of a transparent inline call made fully defined before it expands.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Instantiates the type arguments of `call` (dotty's `isFullyDefined` with
    /// `ForceDegree.flipBottom` on `callTypeArgs`) and the type variables of its implicit
    /// arguments, then substitutes the instances in its types.
    pub fn define_call_types(&mut self, call: TNodeId) -> Result<(), &'static str> {
        let (mut t, mut implicit_args) = (call, Vec::new());
        loop {
            match *self.tpd.tree(t) {
                T::Apply { fun, args } => {
                    let tpe = self.tpd.tpe(fun);
                    let info = self.method_info(tpe).unwrap_or(tpe);
                    if matches!(self.u.types().get(info), Type::Method { implicit: true, .. }) {
                        implicit_args.extend_from_slice(self.tpd.items(args));
                    }
                    t = fun;
                }
                T::TypeApply { fun, args } => {
                    for a in self.tpd.items(args).to_vec() {
                        let tpe = self.fully_defined(self.tpd.tpe(a))?;
                        self.tpd.get_mut(a).tpe = Some(tpe);
                    }
                    t = fun;
                }
                _ => break,
            }
        }
        for a in implicit_args {
            if self.has_union_bounded_var(a) {
                return Err("type variable bounded by a union in an implicit argument");
            }
            self.define_tree(a, true)?;
        }
        self.define_tree(call, false)
    }

    /// Whether a type in `t` has a type variable without instance whose bound is or widens to a
    /// union.
    fn has_union_bounded_var(&mut self, t: TNodeId) -> bool {
        let mut vars = Vec::new();
        if let Some(tpe) = self.tpd.get(t).tpe {
            let tpe = self.zonk(tpe);
            self.u.map_type(tpe, &mut |u, part| {
                if let Type::Var(v) = *u.types().get(part) {
                    vars.push(v);
                }
                None
            });
        }
        for v in vars {
            let (lo, hi) = self.constraint.bounds(v);
            for b in [lo, hi] {
                let b = self.zonk(b);
                let w = self.u.widen(b);
                let d = self.u.dealias(w);
                if matches!(self.u.types().get(d), Type::Or(..)) {
                    return true;
                }
            }
        }
        self.tpd.children(t).into_iter().any(|c| self.has_union_bounded_var(c))
    }

    /// Substitutes the instances of type variables in the types in `t`, instantiating the open
    /// ones if `force`; outside the subset if one stays open.
    pub fn define_tree(&mut self, t: TNodeId, force: bool) -> Result<(), &'static str> {
        let closure = self.tpd.sym(t).filter(|s| self.syms.closure_syms.contains(s));
        if let Some(tpe) = self.tpd.get(t).tpe {
            let tpe = self.defined_type(tpe, force)?;
            self.tpd.get_mut(t).tpe = Some(tpe);
        }
        if let Some(s) = closure {
            let info = self.u.info(s);
            let info = self.defined_type(info, force)?;
            self.u.set_info(s, info);
        }
        for c in self.tpd.children(t) {
            self.define_tree(c, force)?;
        }
        Ok(())
    }

    fn defined_type(&mut self, t: TypeId, force: bool) -> Result<TypeId, &'static str> {
        if force {
            return self.fully_defined(t);
        }
        let t = self.zonk(t);
        match self.is_fully_defined(t) {
            true => Ok(t),
            false => Err("type variable open at a transparent inline call"),
        }
    }
}
