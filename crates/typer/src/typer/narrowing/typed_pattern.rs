// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The GADT bounds a typed pattern gives the selector's type parameters and the types it binds
//! (dotty's `constrainPatternType` in `typedTyped` with the binds of `typedBind` in the GADT
//! constraint).

use super::super::Typer;
use super::super::gadt::{current_bounds, plain_type_ref};
use super::Narrowing;
use super::bounded::{refines_invariantly, without_variant_args};
use super::constrain::constrain_pattern;
use super::order::ordered_bound_type;
use super::vars::narrow;
use tasty::{SymbolId, Type, TypeId, TypeVars, Universe};

impl Narrowing {
    /// Narrows the GADT bounds by type pattern `pat` against `sel` where it does not conform to
    /// `sel` otherwise (dotty's `constrainPatternType` in `typedTyped`).
    pub fn by_type_pattern(&self, t: &mut Typer, pat: TypeId, sel: TypeId) {
        let mark = t.constraint.mark();
        if constrain_pattern(t, pat, sel) == Ok(true) {
            t.constraint.rollback(mark);
            return;
        }
        t.constraint.rollback(mark);
        let sel = this_widened(t.u, sel);
        let sel = match t.is_singleton(pat) || refines_invariantly(t, pat, sel) {
            true => sel,
            false => match without_variant_args(t, sel) {
                Some(widened) => widened,
                None => return,
            },
        };
        let from = t.constraint.count();
        match self.vars(t, (pat, from), sel) {
            Ok(Some(vars)) if narrow(t, &vars).is_ok() => {}
            _ => t.constraint.rollback(mark),
        }
    }

    /// Narrows the GADT bounds by pattern type `pat` binding types `syms` against `sel`, and gives
    /// each bound type the bounds the comparison leaves it; whether it did. Outside the subset
    /// where a bound type's bounds refer to parameters made one, which dotty bounds by each.
    pub fn by_binding_pattern(
        &self,
        t: &mut Typer,
        (pat, sel): (TypeId, TypeId),
        syms: &[SymbolId],
    ) -> Result<bool, &'static str> {
        let mark = t.constraint.mark();
        let from = t.constraint.count();
        let vars: Vec<TypeId> = syms.iter().map(|_| t.constraint.fresh(t.u)).collect();
        let subst = |u: &mut Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Sym(None, s) => syms.iter().position(|&b| b == s).map(|i| vars[i]),
                _ => None,
            })
        };
        for (&s, &v) in syms.iter().zip(&vars) {
            let info = t.u.info(s);
            if let Type::Bounds(lo, hi) = *t.u.types().get(info) {
                let (lo, hi) = (subst(t.u, lo), subst(t.u, hi));
                t.constraint.declare(t.u, v, lo, hi);
            }
        }
        let pat = subst(t.u, pat);
        let narrowed = match self.vars(t, (pat, from), sel) {
            Ok(Some(narrowed)) => narrowed,
            _ => {
                t.constraint.rollback(mark);
                return Ok(false);
            }
        };
        let mut bounds = Vec::new();
        for (&s, &v) in syms.iter().zip(&vars) {
            let Type::Var(id) = *t.u.types().get(v) else { continue };
            let own = t.u.intern_type(Type::Sym(None, s));
            match t.constraint.instance(id) {
                Some(ty) => bounds.push((s, ty, ty)),
                None => {
                    let (lo, hi) = t.constraint.bounds(id);
                    t.constraint.instantiate(id, own);
                    bounds.push((s, lo, hi));
                }
            }
        }
        if narrow(t, &narrowed).is_err() {
            t.constraint.rollback(mark);
            return Ok(false);
        }
        t.gadt.bind(syms);
        let made_one = params_made_one(t.u, &narrowed);
        for (s, lo, hi) in bounds {
            let (lo, hi) = (t.zonk(lo), t.zonk(hi));
            let own = t.u.intern_type(Type::Sym(None, s));
            if lo == own || hi == own || t.types().has_type_var(lo) || t.types().has_type_var(hi) {
                continue;
            }
            if t.refers_to(lo, &made_one) || t.refers_to(hi, &made_one) {
                return Err("type bound by a typed pattern through parameters made one");
            }
            let params: Vec<SymbolId> = narrowed.iter().map(|&(p, _)| p).collect();
            let poly_param = |t: &mut Typer, r: SymbolId| {
                params.contains(&r) && t.u.symbol(r).owner.is_some_and(|o| t.gadt.poly_owners.contains(&o))
            };
            if lo == hi && plain_type_ref(t.u, lo).is_some_and(|r| poly_param(t, r)) {
                return Err("type bound by a typed pattern made one with a polymorphic function's parameter");
            }
            let (lo, hi) = ordered_bound_type(t.u, s, (lo, hi), &params);
            let Some((old_lo, old_hi)) = current_bounds(t.u, s) else { continue };
            let lo = match (t.u.is_subtype(old_lo, lo), t.u.is_subtype(lo, old_lo)) {
                (true, _) => lo,
                (_, true) => old_lo,
                _ => t.u.intern_type(Type::Or(lo, old_lo, false)),
            };
            let hi = match (t.u.is_subtype(hi, old_hi), t.u.is_subtype(old_hi, hi)) {
                (true, _) => hi,
                (_, true) => old_hi,
                _ => t.u.intern_type(Type::And(hi, old_hi)),
            };
            if (lo, hi) == (old_lo, old_hi) {
                continue;
            }
            let info = t.u.intern_type(if lo == hi { Type::Alias(lo) } else { Type::Bounds(lo, hi) });
            t.u.set_info(s, info);
            t.known_pattern_types.insert(s);
        }
        Ok(true)
    }
}

/// Widens `this` of a generic class to the class applied to its type parameters, else gives `t`.
pub fn this_widened(u: &mut Universe, t: TypeId) -> TypeId {
    let Type::This(c) = *u.types().get(t) else { return t };
    match u.class_symbol(c) {
        Some(class) if !u.type_params(class).is_empty() => u.this_type(class),
        _ => t,
    }
}

/// The parameters of `narrowed` whose GADT bounds are another of them, and those others.
fn params_made_one(u: &mut Universe, narrowed: &[(SymbolId, TypeId)]) -> Vec<SymbolId> {
    let params: Vec<SymbolId> = narrowed.iter().map(|&(s, _)| s).collect();
    let mut made_one = Vec::new();
    for &s in &params {
        let Some((lo, hi)) = u.gadt_bounds(s) else { continue };
        if let Some(r) = plain_type_ref(u, hi).filter(|r| lo == hi && params.contains(r)) {
            made_one.extend([s, r]);
        }
    }
    made_one
}
