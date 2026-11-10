// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! The bounds the selector gives the types a typed pattern binds (dotty's
//! `constrainPatternType` on pattern-bound symbols).

use super::super::dependent_results::has_wildcard;
use super::super::type_ops::TypeOps;
use super::super::{Pt, Typer};
use super::Narrowing;
use super::constrain::constrain_pattern;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Narrowing {
    /// Types pattern type tree `tpt` against `pt`, the types it binds bounded by what the
    /// pattern conforming to the selector requires of them, those then known.
    pub fn bounded_pattern_type(&self, t: &mut Typer, tpt: TNodeId, pattern: bool, pt: Pt) -> TypeId {
        let tpe = t.tpd.tpe(tpt);
        let Pt::Type(sel) = pt else { return tpe };
        let mut bound = Vec::new();
        t.pattern_types(tpt, &mut bound, false);
        let syms: Vec<SymbolId> = bound.into_iter().map(|(s, _)| s).collect();
        if pattern && syms.is_empty() {
            self.by_type_pattern(t, tpe, sel);
        }
        if pattern
            && !syms.is_empty()
            && (t.only_against_wildcards(tpe, sel, &syms) || t.has_unapplied_bases(sel))
        {
            t.known_pattern_types.extend(syms);
            return tpe;
        }
        if !pattern || syms.is_empty() || t.is_top(sel) {
            return tpe;
        }
        let sel = match refines_invariantly(t, tpe, sel) {
            true => sel,
            false => match without_variant_args(t, sel) {
                Some(widened) => widened,
                None => {
                    if t.bound_by_variant_args(tpe, sel, &syms) {
                        t.known_pattern_types.extend(syms);
                    }
                    return tpe;
                }
            },
        };
        let mark = t.constraint.mark();
        let vars: Vec<TypeId> = syms.iter().map(|_| t.constraint.fresh(t.u)).collect();
        let subst = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| {
                let Some((None, s)) = u.sym_ref(x) else { return None };
                syms.iter().position(|&b| b == s).map(|i| vars[i])
            })
        };
        let mut ok = true;
        for (&s, &v) in syms.iter().zip(&vars) {
            let info = t.u.info(s);
            if let Some((lo, hi)) = t.u.bounds(info) {
                let (lo, hi) = (subst(t.u, lo), subst(t.u, hi));
                t.constraint.declare(t.u, v, lo, hi);
                ok &= t.conforms(lo, v) && t.conforms(v, hi);
            }
        }
        let pat = subst(t.u, tpe);
        let ok = ok && constrain_pattern(t, pat, sel).unwrap_or(false);
        let bounds: Vec<(TypeId, TypeId)> = match ok {
            true => vars.iter().map(|&v| t.types().var_bounds_of(v)).collect(),
            false => Vec::new(),
        };
        t.constraint.rollback(mark);
        if !ok {
            if let Err(what) = self.by_binding_pattern(t, (tpe, sel), &syms) {
                let span = t.tpd.span(tpt);
                t.unsupported_at(span, what);
            }
            return tpe;
        }
        if bounds.iter().any(|&(lo, hi)| t.types().has_type_var(lo) || t.types().has_type_var(hi)) {
            return tpe;
        }
        for (&s, (lo, hi)) in syms.iter().zip(bounds) {
            let info = match lo == hi {
                true => Type::Alias(lo),
                false => Type::Bounds(lo, hi),
            };
            let info = t.u.intern_type(info);
            t.u.set_info(s, info);
            t.known_pattern_types.insert(s);
        }
        tpe
    }
}

/// Tells whether pattern type `pat` against `sel` constrains every type argument of their
/// common base class: `sel` without wildcard arguments, and `pat`'s class final or a case class,
/// or that base class's parameters invariant (dotty's `assumeInvariantRefinement`, the variant
/// arguments otherwise left unconstrained); never for an opaque type seen from outside.
pub fn refines_invariantly(t: &mut Typer, pat: TypeId, sel: TypeId) -> bool {
    let sel = t.dealiased(sel);
    if let Type::Or(a, b, _) = *t.u.ty(sel) {
        return match (t.types().is_null_type(a), t.types().is_null_type(b)) {
            (true, _) => refines_invariantly(t, pat, b),
            (_, true) => refines_invariantly(t, pat, a),
            _ => refines_invariantly(t, pat, a) && refines_invariantly(t, pat, b),
        };
    }
    let widened = t.u.widen(sel);
    if has_wildcard(t.u, widened) || t.is_opaque_ref(pat) || t.is_opaque_ref(widened) {
        return false;
    }
    let (Some(p), Some(s)) = (t.u.class_symbol(pat), t.u.class_symbol(sel)) else { return false };
    let flags = t.u.symbol(p).flags;
    if flags.has(tasty::tags::FINAL) || flags.has(tasty::tags::CASE) {
        return true;
    }
    let base = if t.u.derives_from(p, s) { s } else { p };
    t.u.variances(base).iter().all(|&v| v == 0)
}

/// Replaces the arguments at variant parameters of class or opaque type application `sel` by
/// `?` and drops its `Null` parts, as `Map[K, V]` becomes `Map[K, ?]`, since
/// `constrainSimplePatternType` leaves those arguments unconstrained; `None` if `sel` has a
/// wildcard argument or no invariant parameter.
pub fn without_variant_args(t: &mut Typer, sel: TypeId) -> Option<TypeId> {
    let sel = t.strip_null(sel);
    let sel = t.dealiased(sel);
    let sel = t.u.widen(sel);
    if has_wildcard(t.u, sel) {
        return None;
    }
    let (tycon, args) = t.u.applied(sel).map(|(tycon, args)| (tycon, args.to_vec()))?;
    let class = match t.is_opaque_ref(tycon) {
        true => t.referenced(tycon)?,
        false => t.u.class_symbol(tycon)?,
    };
    let variances = t.u.variances(class);
    if variances.len() != args.len() || variances.iter().all(|&v| v != 0) {
        return None;
    }
    let any = t.u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY));
    let args = args.iter().zip(&variances).map(|(&a, &v)| if v == 0 { a } else { any }).collect();
    Some(t.u.intern_type(Type::Applied(tycon, args)))
}

impl TypeOps<'_> {
    /// Gives the bounds of type variable `v` zonked, both its instance if it has one.
    fn var_bounds_of(&mut self, v: TypeId) -> (TypeId, TypeId) {
        let Some(id) = self.u.var(v) else { return (v, v) };
        let (lo, hi) = match self.constraint.instance(id) {
            Some(t) => (t, t),
            None => self.constraint.bounds(id),
        };
        (self.zonk(lo), self.zonk(hi))
    }
}
