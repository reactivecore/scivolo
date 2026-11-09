// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! The bounds the selector gives the types a typed pattern binds (dotty's
//! `constrainPatternType` on pattern-bound symbols).

use super::dependent_results::has_wildcard;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The type of pattern type tree `tpt` against `pt`, the types it binds bounded by what the
    /// pattern conforming to the selector requires of them, those then known.
    pub fn bounded_pattern_type(&mut self, tpt: TNodeId, pattern: bool, pt: Pt, ctx: Ctx) -> TypeId {
        let tpe = self.tpd.tpe(tpt);
        let Pt::Type(sel) = pt else { return tpe };
        let mut bound = Vec::new();
        self.pattern_types(tpt, &mut bound, false);
        let syms: Vec<SymbolId> = bound.into_iter().map(|(s, _)| s).collect();
        if pattern && syms.is_empty() {
            self.narrow_by_type_pattern(tpe, sel, ctx);
        }
        if pattern
            && !syms.is_empty()
            && (self.only_against_wildcards(tpe, sel, &syms) || self.has_unapplied_bases(sel))
        {
            self.known_pattern_types.extend(syms);
            return tpe;
        }
        if !pattern || syms.is_empty() || self.is_top(sel) {
            return tpe;
        }
        let sel = match self.refines_invariantly(tpe, sel) {
            true => sel,
            false => match self.without_variant_args(sel) {
                Some(widened) => widened,
                None => {
                    if self.bound_by_variant_args(tpe, sel, &syms) {
                        self.known_pattern_types.extend(syms);
                    }
                    return tpe;
                }
            },
        };
        let mark = self.constraint.mark();
        let vars: Vec<TypeId> = syms.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let subst = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Sym(None, s) => syms.iter().position(|&b| b == s).map(|i| vars[i]),
                _ => None,
            })
        };
        let mut ok = true;
        for (&s, &v) in syms.iter().zip(&vars) {
            let info = self.u.info(s);
            if let Type::Bounds(lo, hi) = *self.u.types().get(info) {
                let (lo, hi) = (subst(self.u, lo), subst(self.u, hi));
                self.constraint.declare(self.u, v, lo, hi);
                ok &= self.conforms(lo, v) && self.conforms(v, hi);
            }
        }
        let pat = subst(self.u, tpe);
        let ok = ok && self.constrain_pattern(pat, sel).unwrap_or(false);
        let bounds: Vec<(TypeId, TypeId)> = match ok {
            true => vars.iter().map(|&v| self.var_bounds_of(v)).collect(),
            false => Vec::new(),
        };
        self.constraint.rollback(mark);
        let open = |t: &mut Self, x: TypeId| {
            let mut found = false;
            t.u.map_type(x, &mut |u, p| {
                found |= matches!(u.types().get(p), Type::Var(_));
                None
            });
            found
        };
        if !ok {
            if let Err(what) = self.narrow_by_binding_pattern(tpe, sel, &syms, ctx) {
                let span = self.tpd.span(tpt);
                self.unsupported_at(span, what);
            }
            return tpe;
        }
        if bounds.iter().any(|&(lo, hi)| open(self, lo) || open(self, hi)) {
            return tpe;
        }
        for (&s, (lo, hi)) in syms.iter().zip(bounds) {
            let info = match lo == hi {
                true => Type::Alias(lo),
                false => Type::Bounds(lo, hi),
            };
            let info = self.u.intern_type(info);
            self.u.set_info(s, info);
            self.known_pattern_types.insert(s);
        }
        tpe
    }

    /// Whether pattern type `pat` against `sel` constrains every type argument of their common
    /// base class: `sel` without wildcard arguments, and `pat`'s class final or a case class, or
    /// that base class's parameters invariant (dotty's `assumeInvariantRefinement`, the variant
    /// arguments otherwise left unconstrained); never for an opaque type seen from outside.
    pub fn refines_invariantly(&mut self, pat: TypeId, sel: TypeId) -> bool {
        let sel = self.dealiased(sel);
        if let Type::Or(a, b, _) = *self.u.types().get(sel) {
            return match (self.is_null_type(a), self.is_null_type(b)) {
                (true, _) => self.refines_invariantly(pat, b),
                (_, true) => self.refines_invariantly(pat, a),
                _ => self.refines_invariantly(pat, a) && self.refines_invariantly(pat, b),
            };
        }
        let widened = self.u.widen(sel);
        if has_wildcard(self.u, widened) || self.is_opaque_ref(pat) || self.is_opaque_ref(widened) {
            return false;
        }
        let (Some(p), Some(s)) = (self.u.class_symbol(pat), self.u.class_symbol(sel)) else { return false };
        let flags = self.u.symbol(p).flags;
        if flags.has(tasty::tags::FINAL) || flags.has(tasty::tags::CASE) {
            return true;
        }
        let base = if self.u.derives_from(p, s) { s } else { p };
        self.u.variances(base).iter().all(|&v| v == 0)
    }

    /// Class or opaque type application `sel` with its arguments at variant parameters replaced by `?`, which
    /// `constrainSimplePatternType` leaves unconstrained, its `Null` parts dropped; `None` if `sel` has a wildcard
    /// argument or no invariant parameter.
    pub fn without_variant_args(&mut self, sel: TypeId) -> Option<TypeId> {
        let sel = self.strip_null(sel);
        let sel = self.dealiased(sel);
        let sel = self.u.widen(sel);
        if has_wildcard(self.u, sel) {
            return None;
        }
        let Type::Applied(tycon, args) = self.u.types().get(sel).clone() else { return None };
        let class = match self.is_opaque_ref(tycon) {
            true => self.referenced(tycon)?,
            false => self.u.class_symbol(tycon)?,
        };
        let variances = self.u.variances(class);
        if variances.len() != args.len() || variances.iter().all(|&v| v != 0) {
            return None;
        }
        let any = self.u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY));
        let args = args.iter().zip(&variances).map(|(&a, &v)| if v == 0 { a } else { any }).collect();
        Some(self.u.intern_type(Type::Applied(tycon, args)))
    }

    /// Whether pattern type `pat` and `sel` apply the same class, each of `syms` an argument of
    /// `pat` only where `sel` has an unbounded wildcard, which constrains none of them.
    fn only_against_wildcards(&mut self, pat: TypeId, sel: TypeId, syms: &[SymbolId]) -> bool {
        let sel = self.dealiased(sel);
        let sel = self.u.widen(sel);
        let (Type::Applied(pc, pargs), Type::Applied(sc, sargs)) =
            (self.u.types().get(pat).clone(), self.u.types().get(sel).clone())
        else {
            return false;
        };
        let same = self.u.class_symbol(pc).is_some_and(|c| self.u.class_symbol(sc) == Some(c));
        let b = self.u.builtins();
        let (any, nothing) = (b.any, b.nothing);
        same && pargs.len() == sargs.len()
            && pargs.iter().zip(&sargs).all(|(&p, &s)| match *self.u.types().get(p) {
                Type::Sym(None, b) if syms.contains(&b) => match *self.u.types().get(s) {
                    Type::Bounds(lo, hi) => {
                        self.u.class_symbol(lo) == Some(nothing) && self.u.class_symbol(hi) == Some(any)
                    }
                    _ => false,
                },
                _ => !self.refers_to(p, syms),
            })
    }

    /// Whether `sel` is a class type whose base classes have no type parameters, against which a
    /// pattern's type constrains none of its bound types.
    fn has_unapplied_bases(&mut self, sel: TypeId) -> bool {
        let sel = self.dealiased(sel);
        let sel = self.u.widen(sel);
        let Type::Sym(_, c) = *self.u.types().get(sel) else { return false };
        self.u.is_class(c) && self.u.linearization(c).into_iter().all(|b| self.u.type_params(b).is_empty())
    }

    /// The bounds of type variable `v` zonked, both its instance if it has one.
    fn var_bounds_of(&mut self, v: TypeId) -> (TypeId, TypeId) {
        let Type::Var(id) = *self.u.types().get(v) else { return (v, v) };
        let (lo, hi) = match self.constraint.instance(id) {
            Some(t) => (t, t),
            None => self.constraint.bounds(id),
        };
        (self.zonk(lo), self.zonk(hi))
    }
}
