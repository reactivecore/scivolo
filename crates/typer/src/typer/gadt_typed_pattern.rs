// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The GADT bounds a typed pattern binding types gives the selector's type parameters and the
//! bound types (dotty's `constrainPatternType` in `typedTyped` with the binds of `typedBind` in
//! the GADT constraint).

use super::Typer;
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Narrows the GADT bounds by pattern type `pat` binding types `syms` against `sel`, and gives
    /// each bound type the bounds the comparison leaves it; whether it did.
    pub fn narrow_by_binding_pattern(
        &mut self,
        pat: TypeId,
        sel: TypeId,
        syms: &[SymbolId],
        ctx: Ctx,
    ) -> bool {
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
                return false;
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
            return false;
        }
        self.gadt_bind(syms);
        for (s, lo, hi) in bounds {
            let (lo, hi) = (self.zonk(lo), self.zonk(hi));
            let own = self.u.intern_type(Type::Sym(None, s));
            if lo == own || hi == own || self.has_type_var(lo) || self.has_type_var(hi) {
                continue;
            }
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
        true
    }
}
