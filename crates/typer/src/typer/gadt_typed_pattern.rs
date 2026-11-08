// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The GADT bounds a typed pattern gives the selector's type parameters and the types it binds
//! (dotty's `constrainPatternType` in `typedTyped` with the binds of `typedBind` in the GADT
//! constraint).

use super::Typer;
use super::gadt::plain_type_ref;
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Narrows the GADT bounds by type pattern `pat` against `sel` where it does not conform to
    /// `sel` otherwise (dotty's `constrainPatternType` in `typedTyped`).
    pub fn narrow_by_type_pattern(&mut self, pat: TypeId, sel: TypeId, ctx: Ctx) {
        let mark = self.constraint.mark();
        if self.constrain_pattern(pat, sel) == Ok(true) {
            self.constraint.rollback(mark);
            return;
        }
        self.constraint.rollback(mark);
        let sel = self.this_widened(sel);
        let sel = match self.is_singleton(pat) || self.refines_invariantly(pat, sel) {
            true => sel,
            false => match self.without_variant_args(sel) {
                Some(widened) => widened,
                None => return,
            },
        };
        let from = self.constraint.count();
        match self.gadt_vars((pat, from), sel, ctx) {
            Ok(Some(vars)) if self.narrow_gadt(&vars).is_ok() => {}
            _ => self.constraint.rollback(mark),
        }
    }

    /// `this` of a class widened to the class applied to its type parameters, else `t`.
    pub fn this_widened(&mut self, t: TypeId) -> TypeId {
        let Type::This(c) = *self.u.types().get(t) else { return t };
        match self.u.class_symbol(c) {
            Some(class) if !self.u.type_params(class).is_empty() => self.u.this_type(class),
            _ => t,
        }
    }

    /// Narrows the GADT bounds by pattern type `pat` binding types `syms` against `sel`, and gives
    /// each bound type the bounds the comparison leaves it; whether it did. Outside the subset
    /// where a bound type's bounds refer to parameters made one, which dotty bounds by each.
    pub fn narrow_by_binding_pattern(
        &mut self,
        pat: TypeId,
        sel: TypeId,
        syms: &[SymbolId],
        ctx: Ctx,
    ) -> Result<bool, &'static str> {
        let mark = self.constraint.mark();
        let from = self.constraint.count();
        let vars: Vec<TypeId> = syms.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let subst = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Sym(None, s) => syms.iter().position(|&b| b == s).map(|i| vars[i]),
                _ => None,
            })
        };
        for (&s, &v) in syms.iter().zip(&vars) {
            let info = self.u.info(s);
            if let Type::Bounds(lo, hi) = *self.u.types().get(info) {
                let (lo, hi) = (subst(self.u, lo), subst(self.u, hi));
                self.constraint.declare(self.u, v, lo, hi);
            }
        }
        let pat = subst(self.u, pat);
        let narrowed = match self.gadt_vars((pat, from), sel, ctx) {
            Ok(Some(narrowed)) => narrowed,
            _ => {
                self.constraint.rollback(mark);
                return Ok(false);
            }
        };
        let mut bounds = Vec::new();
        for (&s, &v) in syms.iter().zip(&vars) {
            let Type::Var(id) = *self.u.types().get(v) else { continue };
            let own = self.u.intern_type(Type::Sym(None, s));
            match self.constraint.instance(id) {
                Some(t) => bounds.push((s, t, t)),
                None => {
                    let (lo, hi) = self.constraint.bounds(id);
                    self.constraint.instantiate(id, own);
                    bounds.push((s, lo, hi));
                }
            }
        }
        if self.narrow_gadt(&narrowed).is_err() {
            self.constraint.rollback(mark);
            return Ok(false);
        }
        self.gadt_bind(syms);
        let made_one = self.params_made_one(&narrowed);
        for (s, lo, hi) in bounds {
            let (lo, hi) = (self.zonk(lo), self.zonk(hi));
            let own = self.u.intern_type(Type::Sym(None, s));
            if lo == own || hi == own || self.has_type_var(lo) || self.has_type_var(hi) {
                continue;
            }
            if self.refers_to(lo, &made_one) || self.refers_to(hi, &made_one) {
                return Err("type bound by a typed pattern through parameters made one");
            }
            let params: Vec<SymbolId> = narrowed.iter().map(|&(p, _)| p).collect();
            let poly_param = |t: &mut Self, r: SymbolId| {
                params.contains(&r) && t.u.symbol(r).owner.is_some_and(|o| t.poly_owners.contains(&o))
            };
            if lo == hi && plain_type_ref(self.u, lo).is_some_and(|r| poly_param(self, r)) {
                return Err("type bound by a typed pattern made one with a polymorphic function's parameter");
            }
            let (lo, hi) = self.ordered_bound_type(s, (lo, hi), &params);
            let Some((old_lo, old_hi)) = self.current_bounds(s) else { continue };
            let lo = match (self.u.is_subtype(old_lo, lo), self.u.is_subtype(lo, old_lo)) {
                (true, _) => lo,
                (_, true) => old_lo,
                _ => self.u.intern_type(Type::Or(lo, old_lo, false)),
            };
            let hi = match (self.u.is_subtype(hi, old_hi), self.u.is_subtype(old_hi, hi)) {
                (true, _) => hi,
                (_, true) => old_hi,
                _ => self.u.intern_type(Type::And(hi, old_hi)),
            };
            if (lo, hi) == (old_lo, old_hi) {
                continue;
            }
            let info = self.u.intern_type(if lo == hi { Type::Alias(lo) } else { Type::Bounds(lo, hi) });
            self.u.set_info(s, info);
            self.known_pattern_types.insert(s);
        }
        Ok(true)
    }

    /// The parameters of `narrowed` whose GADT bounds are another of them, and those others.
    fn params_made_one(&mut self, narrowed: &[(SymbolId, TypeId)]) -> Vec<SymbolId> {
        let params: Vec<SymbolId> = narrowed.iter().map(|&(s, _)| s).collect();
        let mut made_one = Vec::new();
        for &s in &params {
            let Some((lo, hi)) = self.u.gadt_bounds(s) else { continue };
            if let Some(r) = plain_type_ref(self.u, hi).filter(|r| lo == hi && params.contains(r)) {
                made_one.extend([s, r]);
            }
        }
        made_one
    }
}
