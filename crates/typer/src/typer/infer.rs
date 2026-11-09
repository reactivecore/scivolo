// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! Instantiating type variables (dotty's `instanceType`, `isFullyDefined`), and substituting
//! their instances.

use super::{Typer, Widen};
use crate::constraint::Constraint;
use crate::symbols::SCRIPT_BINDERS;
use tasty::{NOTHING, Type, TypeId, TypeVars, Universe, VarId};

/// Whether `t` is a type lambda the script made (bound by a `fresh_binder` of the typer or of
/// the universe).
fn is_script_lambda(u: &Universe, t: TypeId) -> bool {
    matches!(u.types().get(t), Type::Lambda { binder, .. }
        if binder.0 == SCRIPT_BINDERS || binder.0 == Universe::FRESH_BINDERS)
}

/// `t` with the type variables that have instances in `constraint` replaced by them once, a
/// skolem keeping its identity; with `keep`, a variable applied to arguments whose instance is a
/// type lambda stays, its arguments substituted.
fn substituted(u: &mut Universe, c: &Constraint, t: TypeId, changed: &mut bool, keep: bool) -> TypeId {
    u.map_type(t, &mut |u, part| match u.types().get(part).clone() {
        Type::Var(v) => c.instance(v).inspect(|_| *changed = true),
        Type::Skolem(b, info) => {
            let info = substituted(u, c, info, changed, keep);
            Some(u.intern_type(Type::Skolem(b, info)))
        }
        Type::Applied(tycon, args) if keep && lambda_instance(u, c, tycon).is_some() => {
            let args = args.iter().map(|&a| substituted(u, c, a, changed, keep)).collect();
            Some(u.intern_type(Type::Applied(tycon, args)))
        }
        _ => None,
    })
}

/// `t` with instantiated type variables replaced by their instances, normalized; with `keep`, an
/// application of a variable instantiated to a type lambda unreduced.
fn zonked(u: &mut Universe, c: &Constraint, mut t: TypeId, keep: bool) -> TypeId {
    let mut changed = true;
    for _ in 0..16 {
        if !changed {
            break;
        }
        changed = false;
        t = substituted(u, c, t, &mut changed, keep);
    }
    let t = super::seen_from::normalized(u, t);
    if keep { with_lambda_instances(u, c, t) } else { t }
}

/// `t` with each type variable applied to arguments replaced by its instance, a type lambda.
fn with_lambda_instances(u: &mut Universe, c: &Constraint, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let Type::Applied(tycon, args) = u.types().get(part).clone() else { return None };
        let inst = lambda_instance(u, c, tycon)?;
        let args = args.iter().map(|&a| with_lambda_instances(u, c, a)).collect();
        Some(u.intern_type(Type::Applied(inst, args)))
    })
}

/// The instance of type variable `t`, zonked, if it is a type lambda.
fn lambda_instance(u: &mut Universe, c: &Constraint, t: TypeId) -> Option<TypeId> {
    let Type::Var(v) = *u.types().get(t) else { return None };
    let inst = zonked(u, c, c.instance(v)?, false);
    matches!(u.types().get(inst), Type::Lambda { .. }).then_some(inst)
}

impl Typer<'_> {
    /// Whether `a` conforms to `b`, constraining type variables; a stable path also with its
    /// wildcards captured.
    pub fn conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        self.u.is_subtype_in(a, b, &mut self.constraint) || self.conforms_captured_path(a, b)
    }

    /// Whether `a` conforms to `b` frozen, no reference lifted to the `this` of an enclosing
    /// object with opaque types (dotty's test under the root owner).
    pub fn conforms_unlifted(&mut self, a: TypeId, b: TypeId) -> bool {
        let (a, b) = (self.zonk(a), self.zonk(b));
        let owner = self.u.set_comparison_owner(None);
        let ok = self.u.is_subtype(a, b);
        self.u.set_comparison_owner(owner);
        ok
    }

    /// `t` with instantiated type variables replaced by their instances, normalized.
    pub fn zonk(&mut self, t: TypeId) -> TypeId {
        zonked(self.u, &self.constraint, t, false)
    }

    /// `zonk`, an application of a type variable instantiated to a type lambda unreduced (dotty's
    /// tree types keep the `TypeVar`).
    pub fn zonk_kept(&mut self, t: TypeId) -> TypeId {
        zonked(self.u, &self.constraint, t, true)
    }

    /// `v`'s lower bound widened below `v`, which may constrain `v`, or its upper bound (dotty's
    /// `instanceType`).
    pub fn instance_type(&mut self, v: VarId, from_below: bool) -> Result<TypeId, &'static str> {
        let (lo, hi) = self.constraint.bounds(v);
        let inst = if from_below { self.zonk(lo) } else { self.zonk(hi) };
        let inst = self.simplified_inferred(inst);
        if !from_below {
            return Ok(inst);
        }
        let widen = if self.constraint.is_hard(v) { Widen::Singletons } else { Widen::Unions };
        let bound = self.u.intern_type(Type::Var(v));
        self.widen_inferred(inst, bound, widen)
    }

    /// Instantiates `v` to its instance type.
    pub fn instantiate_var(&mut self, v: VarId, from_below: bool) -> Result<(), &'static str> {
        let inst = self.instance_type(v, from_below)?;
        let hi = self.constraint.bounds(v).1;
        let hi = self.zonk(hi);
        if from_below
            && matches!(self.u.types().get(hi), Type::Var(_))
            && !self.is_below_singleton(hi, 0)
            && let widened = self.widen_singletons(inst, false)
            && self.module_ref(widened) != inst
        {
            return Err("singleton instance below another type variable");
        }
        let mut cyclic = false;
        self.u.map_type(inst, &mut |u, part| {
            cyclic |= *u.types().get(part) == Type::Var(v);
            None
        });
        if cyclic {
            return Err("type variable instantiated to a type containing it");
        }
        if let Type::And(a, b) | Type::Or(a, b, _) = *self.u.types().get(inst)
            && (self.type_arity(a) > 0 || self.type_arity(b) > 0)
        {
            return Err("type variable instantiated to an intersection or union of type constructors");
        }
        let script_hk = is_script_lambda(self.u, hi);
        if script_hk && !from_below && self.type_arity(inst) > 0 {
            return Err("higher-kinded type variable instantiated to its upper bound");
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

    /// `t` with every type variable in it instantiated (dotty's `isFullyDefined` with
    /// `ForceDegree.flipBottom`): in the direction its declared bounds give (`instDecision`), else
    /// from below unless its lower bound is `Nothing`.
    pub fn fully_defined(&mut self, t: TypeId) -> Result<TypeId, &'static str> {
        self.fully_defined_as(t, false)
    }

    /// `fully_defined`, with dotty's `ForceDegree.all` if `bottom_ok` (`fullyDefinedType`): the
    /// variables of each pass without a direction by their variances at its start, one with a
    /// lower bound maximized where it occurs contravariantly, one without minimized where it
    /// occurs covariantly, or invariantly without upper bound.
    pub fn fully_defined_as(&mut self, t: TypeId, bottom_ok: bool) -> Result<TypeId, &'static str> {
        for _ in 0..64 {
            let t = self.zonk(t);
            let mut open = Vec::new();
            self.u.map_type(t, &mut |u, part| {
                if let Type::Var(v) = *u.types().get(part)
                    && !open.contains(&v)
                {
                    open.push(v);
                }
                None
            });
            if open.is_empty() {
                return Ok(t);
            }
            let variances = if bottom_ok { self.occurrence_variances(t) } else { Default::default() };
            for v in open {
                if self.constraint.instance(v).is_some() {
                    continue;
                }
                let (lo, hi) = self.constraint.bounds(v);
                let variance = variances.get(&v.0).copied().unwrap_or(0);
                let from_below = match (self.constraint.direction(self.u, v), lo != NOTHING) {
                    (0, true) => variance >= 0,
                    (0, false) => bottom_ok && (variance > 0 || variance == 0 && self.is_exactly_any(hi)),
                    (direction, _) => direction < 0,
                };
                self.instantiate_var(v, from_below)?;
            }
        }
        Err("type variables")
    }
}
