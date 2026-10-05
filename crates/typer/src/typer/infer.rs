//! Instantiating type variables (dotty's `interpolateTypeVars`, `instantiateTypeVars`,
//! `instanceType`, `isFullyDefined`), and substituting their instances.

use super::Typer;
use crate::tpd::TNodeId;
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, VarId};

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
    /// lower bound if they do not occur and have one; invariant ones stay, and all stay while
    /// `tree` is a method not applied to all its parameter clauses.
    pub fn interpolate(&mut self, tree: TNodeId, from: usize) -> Result<(), &'static str> {
        let Some(tpe) = self.tpd.get(tree).tpe else { return Ok(()) };
        if matches!(self.u.types().get(tpe), Type::Method { .. } | Type::Poly { .. }) {
            return Ok(());
        }
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
        self.fully_defined_as(t, false)
    }

    /// `fully_defined` with dotty's `ForceDegree.all` if `bottom_ok`: a variable with a lower
    /// bound maximized where it occurs contravariantly, one without minimized where it occurs
    /// covariantly, or invariantly without upper bound.
    pub fn fully_defined_as(&mut self, t: TypeId, bottom_ok: bool) -> Result<TypeId, &'static str> {
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
            let (lo, hi) = self.constraint.bounds(v);
            let variance = if bottom_ok { self.variances(t, 0).get(&v.0).copied().unwrap_or(0) } else { 0 };
            let from_below = match lo != NOTHING {
                true => variance >= 0,
                false => bottom_ok && (variance > 0 || variance == 0 && hi == ANY),
            };
            self.instantiate_var(v, from_below)?;
        }
        Err("type variables")
    }

    /// Makes the types of all trees, and of closures' symbols, fully defined.
    pub fn zonk_trees(&mut self) -> Result<(), &'static str> {
        for s in self.syms.closure_syms.clone() {
            let info = self.u.info(s);
            let info = self.fully_defined(info)?;
            self.u.set_info(s, info);
        }
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
