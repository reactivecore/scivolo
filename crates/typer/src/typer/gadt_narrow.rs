// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The GADT bounds a pattern's type gives the narrowable type parameters of the selector type
//! (dotty's `constrainPatternType` in `GadtConstraintInference` mode, `narrowGADTBounds`).

use super::Typer;
use super::gadt::plain_type_ref;
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether pattern type `pat` conforms to `sel` with `sel`'s narrowable type parameters
    /// as type variables of their bounds, constraining them; those parameters and variables. A
    /// variable that variables from id `from` on were made one with is instantiated to the first
    /// of their parameters, which takes its instance as bounds, as dotty's comparison constrains a
    /// pattern's type variables first.
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
        fn class_type(u: &mut tasty::Universe, t: TypeId) -> bool {
            let t = u.dealias(t);
            match *u.types().get(t) {
                Type::Applied(tycon, _) => u.class_symbol(tycon).is_some_and(|c| u.is_class(c)),
                Type::Sym(_, c) => u.is_class(c),
                Type::Or(a, b, _) => class_type(u, a) && class_type(u, b),
                _ => false,
            }
        }
        let sel = self.this_widened(sel);
        let core = match *self.u.types().get(sel) {
            Type::Annotated(t, _) => t,
            _ => sel,
        };
        let core = self.u.dealias(core);
        if !class_type(self.u, core) {
            return Ok(None);
        }
        let syms = self.gadt_symbols(sel, ctx);
        if syms.is_empty() {
            return Ok(None);
        }
        let base = self.u.class_symbol(core);
        let syms = self.gadt_related(syms, pat, base, ctx);
        let first = self.constraint.count();
        let vars: Vec<TypeId> = syms.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let subst = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| {
                let s = plain_type_ref(u, x)?;
                syms.iter().position(|&b| b == s).map(|i| vars[i])
            })
        };
        for (&s, &v) in syms.iter().zip(&vars) {
            if let Some((lo, hi)) = self.current_bounds(s) {
                let (lo, hi) = (subst(self.u, lo), subst(self.u, hi));
                self.constraint.declare(self.u, v, lo, hi);
                let (lo_open, hi_open) = (self.has_abstract_type(lo), self.has_abstract_type(hi));
                if !((lo_open || self.conforms(lo, v)) && (hi_open || self.conforms(v, hi))) {
                    return Ok(None);
                }
            }
        }
        let pat = match base.and_then(|c| self.u.base_type(pat, c)) {
            Some(b) if self.u.class_symbol(pat).is_some_and(|c| self.u.is_class(c)) => b,
            _ => pat,
        };
        let (sel, pat) = (subst(self.u, sel), subst(self.u, pat));
        if !self.constrain_pattern(pat, sel)? {
            return Ok(None);
        }
        let reps: Vec<TypeId> = vars.iter().map(|&v| self.var_root(v)).collect();
        let pattern_reps: Vec<TypeId> = (from..first)
            .map(|i| {
                let v = self.u.intern_type(Type::Var(tasty::VarId(i as u32)));
                self.var_root(v)
            })
            .collect();
        let mut narrowed: Vec<(SymbolId, TypeId)> = syms.into_iter().zip(vars.iter().copied()).collect();
        for (i, &r) in reps.iter().enumerate() {
            let Type::Var(p) = *self.u.types().get(r) else { continue };
            let shared =
                r != vars[i] || reps.iter().filter(|&&x| x == r).count() > 1 || pattern_reps.contains(&r);
            if !shared || reps[..i].contains(&r) {
                continue;
            }
            if let Some(t) = self.constraint.instance(p) {
                if !pattern_reps.contains(&r) {
                    continue;
                }
                let fixed = self.constraint.fresh(self.u);
                let Type::Var(f) = *self.u.types().get(fixed) else { unreachable!("a fresh variable") };
                self.constraint.instantiate(f, t);
                narrowed[i].1 = fixed;
            }
            let sym = self.gadt_ref(narrowed[i].0);
            self.constraint.instantiate(p, sym);
        }
        Ok(Some(narrowed))
    }

    /// The variable `t` was made one with, or `t`.
    fn var_root(&mut self, mut t: TypeId) -> TypeId {
        while let Type::Var(id) = *self.u.types().get(t)
            && let Some(next) = self.constraint.instance(id)
            && matches!(self.u.types().get(next), Type::Var(_))
        {
            t = next;
        }
        t
    }

    /// Gives each symbol of `vars` its variable's bounds, which must be known, while the case is
    /// typed; the variables instantiated to their symbols.
    pub fn narrow_gadt(&mut self, vars: &[(SymbolId, TypeId)]) -> Result<(), &'static str> {
        let mut bounds = Vec::new();
        for &(s, v) in vars {
            let Type::Var(mut id) = *self.u.types().get(v) else { continue };
            let (lo, hi) = loop {
                match self.constraint.instance(id).map(|t| (t, self.u.types().get(t).clone())) {
                    Some((_, Type::Var(next))) => id = next,
                    Some((t, _)) if plain_type_ref(self.u, t) != Some(s) => break (t, t),
                    _ => break self.constraint.bounds(id),
                }
            };
            bounds.push((s, lo, hi));
        }
        for &(s, v) in vars {
            let Type::Var(id) = *self.u.types().get(v) else { continue };
            if self.constraint.instance(id).is_none() {
                let sym = self.gadt_ref(s);
                self.constraint.instantiate(id, sym);
            }
        }
        let mut narrowed = Vec::new();
        for (s, lo, hi) in bounds {
            let (lo, hi) = (self.zonk(lo), self.zonk(hi));
            let (lo, hi) = (self.u.dealias(lo), self.u.dealias(hi));
            if [lo, hi].iter().any(|&t| self.has_type_var(t)) {
                return Err("GADT bound of an undetermined type");
            }
            let Some((old_lo, old_hi)) = self.current_bounds(s) else { continue };
            let lo = if plain_type_ref(self.u, lo) == Some(s) { old_lo } else { lo };
            let hi = if plain_type_ref(self.u, hi) == Some(s) { old_hi } else { hi };
            let hi = if hi == old_hi { hi } else { self.gadt_upper_pairs(hi) };
            if (lo, hi) != (old_lo, old_hi) {
                narrowed.push((s, lo, hi));
            }
        }
        let syms: Vec<SymbolId> = vars.iter().map(|&(s, _)| s).collect();
        self.order_gadt(&mut narrowed, &syms);
        for (s, lo, hi) in narrowed {
            self.set_gadt_bounds(s, (lo, hi));
        }
        Ok(())
    }

    /// Whether `t` refers to a type variable.
    pub fn has_type_var(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, p| {
            found |= matches!(u.types().get(p), Type::Var(_));
            None
        });
        found
    }

    /// Whether `t` refers to an abstract type.
    fn has_abstract_type(&mut self, t: TypeId) -> bool {
        let mut parts = Vec::new();
        self.u.map_type(t, &mut |u, p| {
            if matches!(u.types().get(p), Type::Sym(..) | Type::TypeRef(..)) {
                parts.push(p);
            }
            None
        });
        parts.into_iter().any(|p| {
            let s = match *self.u.types().get(p) {
                Type::Sym(_, s) => Some(s),
                _ => self.u.resolve_ref(p),
            };
            s.is_some_and(|s| {
                let info = self.u.info(s);
                self.u.is_type_symbol(s) && matches!(self.u.types().get(info), Type::Bounds(..))
            })
        })
    }
}
