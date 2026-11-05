// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The GADT bounds a pattern's type gives the narrowable type parameters of the selector type
//! (dotty's `constrainPatternType` in `GadtConstraintInference` mode, `narrowGADTBounds`).

use super::Typer;
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether pattern type `pat` conforms to `sel` with `sel`'s narrowable type parameters
    /// as type variables of their bounds, constraining them; those parameters and variables. A
    /// variable that variables from id `from` on were made one with is instantiated to the first
    /// of their parameters, as dotty's comparison constrains a pattern's type variables first.
    pub fn gadt_vars(
        &mut self,
        (pat, from): (TypeId, usize),
        sel: TypeId,
        ctx: Ctx,
    ) -> Result<Option<Vec<(SymbolId, TypeId)>>, &'static str> {
        if let Type::Sym(None, s) = *self.u.types().get(sel)
            && let Some((_, hi)) = self.u.gadt_bounds(s)
        {
            let mark = self.constraint.mark();
            if self.constrain_pattern(pat, hi)? {
                return Ok(Some(Vec::new()));
            }
            self.constraint.rollback(mark);
        }
        let class_type = |u: &mut tasty::Universe, t: TypeId| match *u.types().get(t) {
            Type::Applied(tycon, _) => u.class_symbol(tycon).is_some_and(|c| u.is_class(c)),
            Type::Sym(_, c) => u.is_class(c),
            _ => false,
        };
        let pat_type = self.u.dealias(pat);
        let pat_type = self.u.widen(pat_type);
        let core = match *self.u.types().get(sel) {
            Type::Annotated(t, _) => t,
            _ => sel,
        };
        let core = self.u.dealias(core);
        if !class_type(self.u, core) || matches!(self.u.types().get(pat_type), Type::Or(..)) {
            return Ok(None);
        }
        let syms = self.gadt_symbols(sel, ctx);
        if syms.is_empty() {
            return Ok(None);
        }
        let first = self.constraint.count();
        let vars: Vec<TypeId> = syms.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let subst = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Sym(None, s) => syms.iter().position(|&b| b == s).map(|i| vars[i]),
                _ => None,
            })
        };
        for (&s, &v) in syms.iter().zip(&vars) {
            if let Some((lo, hi)) = self.current_bounds(s) {
                let (lo, hi) = (subst(self.u, lo), subst(self.u, hi));
                self.constraint.declare(self.u, v, lo, hi);
            }
        }
        let sel = subst(self.u, sel);
        if !self.constrain_pattern(pat, sel)? {
            return Ok(None);
        }
        let reps: Vec<TypeId> = vars.iter().map(|&v| self.zonk(v)).collect();
        let pattern_reps: Vec<TypeId> = (from..first)
            .map(|i| {
                let v = self.u.intern_type(Type::Var(tasty::VarId(i as u32)));
                self.zonk(v)
            })
            .collect();
        for (i, &r) in reps.iter().enumerate() {
            let Type::Var(p) = *self.u.types().get(r) else { continue };
            let shared = r != vars[i]
                || reps.iter().filter(|&&x| x == r).count() > 1
                || pattern_reps.iter().any(|&x| x == r);
            if shared && self.constraint.instance(p).is_none() {
                let sym = self.u.intern_type(Type::Sym(None, syms[i]));
                self.constraint.instantiate(p, sym);
            }
        }
        Ok(Some(syms.into_iter().zip(vars).collect()))
    }

    /// Gives each symbol of `vars` its variable's bounds, which must be known, while the case is
    /// typed; the variables instantiated to their symbols.
    pub fn narrow_gadt(&mut self, vars: &[(SymbolId, TypeId)]) -> Result<(), &'static str> {
        let mut bounds = Vec::new();
        for &(s, v) in vars {
            let Type::Var(id) = *self.u.types().get(v) else { continue };
            let (lo, hi) = self.constraint.instance(id).map_or_else(|| self.constraint.bounds(id), |t| (t, t));
            bounds.push((s, lo, hi));
        }
        for &(s, v) in vars {
            let Type::Var(id) = *self.u.types().get(v) else { continue };
            if self.constraint.instance(id).is_none() {
                let sym = self.u.intern_type(Type::Sym(None, s));
                self.constraint.instantiate(id, sym);
            }
        }
        let mut narrowed = Vec::new();
        for (s, lo, hi) in bounds {
            let (lo, hi) = (self.zonk(lo), self.zonk(hi));
            if [lo, hi].iter().any(|&t| self.has_type_var(t)) {
                return Err("GADT bound of an undetermined type");
            }
            let Some((old_lo, old_hi)) = self.current_bounds(s) else { continue };
            let own = self.u.intern_type(Type::Sym(None, s));
            let lo = if lo == own { old_lo } else { lo };
            let hi = if hi == own { old_hi } else { hi };
            if (lo, hi) != (old_lo, old_hi) {
                narrowed.push((s, lo, hi));
            }
        }
        for (s, lo, hi) in narrowed {
            self.set_gadt_bounds(s, (lo, hi));
        }
        Ok(())
    }

    /// Narrows the GADT bounds by type pattern `pat` against `sel` where it does not conform to
    /// `sel` otherwise (dotty's `constrainPatternType` in `typedTyped`).
    pub fn narrow_by_type_pattern(&mut self, pat: TypeId, sel: TypeId, ctx: Ctx) {
        let mark = self.constraint.mark();
        if self.constrain_pattern(pat, sel).unwrap_or(true) {
            self.constraint.rollback(mark);
            return;
        }
        self.constraint.rollback(mark);
        let sel = match self.refines_invariantly(pat, sel) {
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

    /// Whether `t` refers to a type variable.
    fn has_type_var(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, p| {
            found |= matches!(u.types().get(p), Type::Var(_));
            None
        });
        found
    }
}
