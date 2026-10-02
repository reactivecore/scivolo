//! Instantiating type variables (dotty's `interpolateTypeVars`, `instantiateTypeVars`,
//! `instanceType`, `isFullyDefined`), and substituting their instances.

use super::Typer;
use crate::tpd::TNodeId;
use tasty::{NOTHING, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Whether `a` conforms to `b`, constraining type variables.
    pub fn conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        self.u.is_subtype_in(a, b, &mut self.constraint)
    }

    /// `t` with instantiated type variables replaced by their instances, normalized.
    pub fn zonk(&mut self, t: TypeId) -> TypeId {
        let constraint = &self.constraint;
        let mut changed = true;
        let mut t = t;
        for _ in 0..16 {
            if !changed {
                break;
            }
            changed = false;
            t = self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
                Type::Var(v) => constraint.instance(v).inspect(|_| changed = true),
                _ => None,
            });
        }
        self.normalized(t)
    }

    /// Instantiates the type variables of id `from` or later that `tree`'s type determines:
    /// minimized where they occur covariantly, maximized where contravariantly, by their
    /// lower bound if they do not occur and have one; invariant ones stay.
    pub fn interpolate(&mut self, tree: TNodeId, from: usize) -> Result<(), &'static str> {
        let Some(tpe) = self.tpd.get(tree).tpe else { return Ok(()) };
        let variances = self.variances(tpe, from);
        for i in from..self.constraint.count() {
            let v = VarId(i as u32);
            if self.constraint.instance(v).is_some() {
                continue;
            }
            let from_below = match variances.get(&v.0) {
                Some(1) => true,
                Some(-1) => false,
                Some(_) => continue,
                None => self.constraint.bounds(v).0 != NOTHING,
            };
            self.instantiate_var(v, from_below)?;
        }
        Ok(())
    }

    /// Instantiates `v` to its lower bound widened (dotty's `widenInferred`), or its upper bound.
    fn instantiate_var(&mut self, v: VarId, from_below: bool) -> Result<(), &'static str> {
        let (lo, hi) = self.constraint.bounds(v);
        let (lo, hi) = (self.zonk(lo), self.zonk(hi));
        let mut inst = if from_below { lo } else { hi };
        if from_below && !self.is_singleton(hi) {
            let widened = self.widen_singleton(inst);
            if self.conforms(widened, hi) {
                inst = self.module_ref(widened);
            }
        }
        if matches!(self.u.types().get(inst), Type::Or(..)) {
            return Err("type variable instantiated to a union");
        }
        let mut cyclic = false;
        self.u.map_type(inst, &mut |u, part| {
            cyclic |= *u.types().get(part) == Type::Var(v);
            None
        });
        if cyclic {
            return Err("type variable instantiated to a type containing it");
        }
        self.constraint.instantiate(v, inst);
        Ok(())
    }

    /// `t` with every type variable in it instantiated, from below unless its lower bound is
    /// `Nothing` (dotty's `fullyDefinedType` with `flipBottom`).
    pub fn fully_defined(&mut self, t: TypeId) -> Result<TypeId, &'static str> {
        for _ in 0..64 {
            let t = self.zonk(t);
            let mut open = None;
            self.u.map_type(t, &mut |u, part| {
                if let Type::Var(v) = *u.types().get(part) {
                    open = open.or(Some(v));
                }
                None
            });
            let Some(v) = open else { return Ok(t) };
            let from_below = self.constraint.bounds(v).0 != NOTHING;
            self.instantiate_var(v, from_below)?;
        }
        Err("type variables")
    }

    /// Makes the types of all trees fully defined.
    pub fn zonk_trees(&mut self) -> Result<(), &'static str> {
        for i in 0..self.tpd.len() {
            let id = TNodeId::new(i);
            if let Some(t) = self.tpd.get(id).tpe {
                let t = self.fully_defined(t)?;
                self.tpd.get_mut(id).tpe = Some(t);
            }
        }
        Ok(())
    }
}
