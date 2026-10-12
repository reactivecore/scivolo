//! Instantiating type variables (dotty's `instanceType`, `isFullyDefined`), and substituting
//! their instances.

use super::{Typer, Widen};
use crate::constraint::Constraint;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, Universe, VarId};

/// `t` with the type variables that have instances in `constraint` replaced by them once, a
/// skolem keeping its identity.
fn substituted(u: &mut Universe, constraint: &Constraint, t: TypeId, changed: &mut bool) -> TypeId {
    u.map_type(t, &mut |u, part| match *u.types().get(part) {
        Type::Var(v) => constraint.instance(v).inspect(|_| *changed = true),
        Type::Skolem(b, info) => {
            let info = substituted(u, constraint, info, changed);
            Some(u.intern_type(Type::Skolem(b, info)))
        }
        _ => None,
    })
}

impl Typer<'_> {
    /// Whether `a` conforms to `b`, constraining type variables.
    pub fn conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        self.u.is_subtype_in(a, b, &mut self.constraint)
    }

    /// `t` with instantiated type variables replaced by their instances, normalized.
    pub fn zonk(&mut self, t: TypeId) -> TypeId {
        let mut changed = true;
        let mut t = t;
        for _ in 0..16 {
            if !changed {
                break;
            }
            changed = false;
            t = substituted(self.u, &self.constraint, t, &mut changed);
        }
        self.normalized(t)
    }

    /// Instantiates `v` to its lower bound widened (dotty's `instanceType`), or its upper bound.
    pub fn instantiate_var(&mut self, v: VarId, from_below: bool) -> Result<(), &'static str> {
        let (lo, hi) = self.constraint.bounds(v);
        let (lo, hi) = (self.zonk(lo), self.zonk(hi));
        let mut inst = if from_below { lo } else { hi };
        inst = self.simplified(inst);
        if from_below {
            let widen = if self.constraint.is_hard(v) { Widen::Singletons } else { Widen::Unions };
            inst = self.widen_inferred(inst, hi, widen)?;
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

    /// Whether `t` has no uninstantiated type variables (dotty's `isFullyDefined` with
    /// `ForceDegree.none`).
    pub fn is_fully_defined(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        let mut open = false;
        self.u.map_type(t, &mut |u, part| {
            open |= matches!(u.types().get(part), Type::Var(_));
            None
        });
        !open
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
            let variance =
                if bottom_ok { self.occurrence_variances(t).get(&v.0).copied().unwrap_or(0) } else { 0 };
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
            let defines = matches!(self.tpd.tree(id), T::ValDef { .. } | T::DefDef { .. } | T::Bind { .. });
            if let Some(s) = self.tpd.sym(id).filter(|_| defines)
                && !self.u.symbol(s).flags.has(tasty::tags::INLINEPROXY)
            {
                let info = self.u.info(s);
                if super::deskolemize::has_skolem(self.u, info) {
                    return Err("skolem in the type of a definition");
                }
            }
        }
        Ok(())
    }
}
