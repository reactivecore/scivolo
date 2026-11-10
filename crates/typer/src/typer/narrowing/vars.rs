// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The GADT bounds a pattern's type gives the narrowable type parameters of the selector type
//! (dotty's `constrainPatternType` in `GadtConstraintInference` mode, `narrowGADTBounds`).

use super::super::Typer;
use super::super::gadt::{current_bounds, gadt_ref, plain_type_ref};
use super::super::type_ops::TypeOps;
use super::Narrowing;
use super::constrain::constrain_pattern;
use super::order::{Narrowed, order};
use super::tuples::upper_pairs;
use super::typed_pattern::this_widened;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Narrowing {
    /// Whether pattern type `pat` conforms to `sel` with `sel`'s narrowable type parameters
    /// as type variables of their bounds, constraining them; those parameters and variables. A
    /// variable that variables from id `from` on were made one with is instantiated to the first
    /// of their parameters, which takes its instance as bounds, as dotty's comparison constrains a
    /// pattern's type variables first.
    pub fn vars(
        &self,
        t: &mut Typer,
        (pat, from): (TypeId, usize),
        sel: TypeId,
    ) -> Result<Option<Vec<(SymbolId, TypeId)>>, &'static str> {
        if let Type::Sym(None, s) = *t.u.types().get(sel)
            && let Some((_, hi)) = t.u.gadt_bounds(s)
        {
            let mark = t.constraint.mark();
            if constrain_pattern(t, pat, hi)? {
                return Ok(Some(Vec::new()));
            }
            t.constraint.rollback(mark);
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
        let sel = this_widened(t.u, sel);
        let core = match *t.u.types().get(sel) {
            Type::Annotated(t, _) => t,
            _ => sel,
        };
        let core = t.u.dealias(core);
        if !class_type(t.u, core) {
            return Ok(None);
        }
        let syms = t.gadt.symbols(t.u, sel, self.ctx);
        if syms.is_empty() {
            return Ok(None);
        }
        let base = t.u.class_symbol(core);
        let syms = t.gadt.related(t.u, syms, (pat, base), self.ctx);
        let first = t.constraint.count();
        let vars: Vec<TypeId> = syms.iter().map(|_| t.constraint.fresh(t.u)).collect();
        let subst = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| {
                let s = plain_type_ref(u, x)?;
                syms.iter().position(|&b| b == s).map(|i| vars[i])
            })
        };
        for (&s, &v) in syms.iter().zip(&vars) {
            if let Some((lo, hi)) = current_bounds(t.u, s) {
                let (lo, hi) = (subst(t.u, lo), subst(t.u, hi));
                t.constraint.declare(t.u, v, lo, hi);
                let (lo_open, hi_open) = (has_abstract_type(t.u, lo), has_abstract_type(t.u, hi));
                if !((lo_open || t.conforms(lo, v)) && (hi_open || t.conforms(v, hi))) {
                    return Ok(None);
                }
            }
        }
        let pat = match base.and_then(|c| t.u.base_type(pat, c)) {
            Some(b) if t.u.class_symbol(pat).is_some_and(|c| t.u.is_class(c)) => b,
            _ => pat,
        };
        let (sel, pat) = (subst(t.u, sel), subst(t.u, pat));
        if !constrain_pattern(t, pat, sel)? {
            return Ok(None);
        }
        let reps: Vec<TypeId> = vars.iter().map(|&v| t.types().var_root(v)).collect();
        let pattern_reps: Vec<TypeId> = (from..first)
            .map(|i| {
                let v = t.u.intern_type(Type::Var(tasty::VarId(i as u32)));
                t.types().var_root(v)
            })
            .collect();
        let mut narrowed: Vec<(SymbolId, TypeId)> = syms.into_iter().zip(vars.iter().copied()).collect();
        for (i, &r) in reps.iter().enumerate() {
            let Type::Var(p) = *t.u.types().get(r) else { continue };
            let shared =
                r != vars[i] || reps.iter().filter(|&&x| x == r).count() > 1 || pattern_reps.contains(&r);
            if !shared || reps[..i].contains(&r) {
                continue;
            }
            if let Some(ty) = t.constraint.instance(p) {
                if !pattern_reps.contains(&r) {
                    continue;
                }
                let fixed = t.constraint.fresh(t.u);
                let Type::Var(f) = *t.u.types().get(fixed) else { unreachable!("a fresh variable") };
                t.constraint.instantiate(f, ty);
                narrowed[i].1 = fixed;
            }
            let sym = gadt_ref(t.u, narrowed[i].0);
            t.constraint.instantiate(p, sym);
        }
        Ok(Some(narrowed))
    }
}

/// Gives each symbol of `vars` its variable's bounds, which must be known, while the case is
/// typed; the variables instantiated to their symbols.
pub fn narrow(t: &mut Typer, vars: &[(SymbolId, TypeId)]) -> Result<(), &'static str> {
    let mut bounds = Vec::new();
    for &(s, v) in vars {
        let Type::Var(mut id) = *t.u.types().get(v) else { continue };
        let (lo, hi) = loop {
            match t.constraint.instance(id).map(|ty| (ty, t.u.types().get(ty).clone())) {
                Some((_, Type::Var(next))) => id = next,
                Some((ty, _)) if plain_type_ref(t.u, ty) != Some(s) => break (ty, ty),
                _ => break t.constraint.bounds(id),
            }
        };
        bounds.push((s, lo, hi));
    }
    for &(s, v) in vars {
        let Type::Var(id) = *t.u.types().get(v) else { continue };
        if t.constraint.instance(id).is_none() {
            let sym = gadt_ref(t.u, s);
            t.constraint.instantiate(id, sym);
        }
    }
    let mut narrowed = Vec::new();
    for (s, lo, hi) in bounds {
        let (lo, hi) = (t.zonk(lo), t.zonk(hi));
        let (lo, hi) = (t.u.dealias(lo), t.u.dealias(hi));
        if [lo, hi].iter().any(|&ty| t.types().has_type_var(ty)) {
            return Err("GADT bound of an undetermined type");
        }
        let Some((old_lo, old_hi)) = current_bounds(t.u, s) else { continue };
        let lo = if plain_type_ref(t.u, lo) == Some(s) { old_lo } else { lo };
        let hi = if plain_type_ref(t.u, hi) == Some(s) { old_hi } else { hi };
        let hi = if hi == old_hi { hi } else { upper_pairs(t, hi) };
        if (lo, hi) != (old_lo, old_hi) {
            narrowed.push(Narrowed { sym: s, lo, hi });
        }
    }
    let syms: Vec<SymbolId> = vars.iter().map(|&(s, _)| s).collect();
    order(t.u, &mut narrowed, &syms);
    for n in narrowed {
        t.gadt.narrow(t.u, n.sym, (n.lo, n.hi));
    }
    Ok(())
}

impl TypeOps<'_> {
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
}

/// Whether `t` refers to an abstract type.
fn has_abstract_type(u: &mut tasty::Universe, t: TypeId) -> bool {
    let mut parts = Vec::new();
    u.map_type(t, &mut |u, p| {
        if matches!(u.types().get(p), Type::Sym(..) | Type::TypeRef(..)) {
            parts.push(p);
        }
        None
    });
    parts.into_iter().any(|p| {
        let s = match *u.types().get(p) {
            Type::Sym(_, s) => Some(s),
            _ => u.resolve_ref(p),
        };
        s.is_some_and(|s| {
            let info = u.info(s);
            u.is_type_symbol(s) && matches!(u.types().get(info), Type::Bounds(..))
        })
    })
}
