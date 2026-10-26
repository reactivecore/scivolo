// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! The bounds the selector gives the types a typed pattern binds (dotty's
//! `constrainPatternType` on pattern-bound symbols).

use super::dependent_results::has_wildcard;
use super::{Pt, Typer};
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The type of pattern type tree `tpt` against `pt`, the types it binds bounded by what the
    /// pattern conforming to the selector requires of them, those then known.
    pub fn bounded_pattern_type(&mut self, tpt: TNodeId, pattern: bool, pt: Pt) -> TypeId {
        let tpe = self.tpd.tpe(tpt);
        let Pt::Type(sel) = pt else { return tpe };
        let mut bound = Vec::new();
        self.pattern_types(tpt, &mut bound, false);
        if !pattern || bound.is_empty() || self.is_top(sel) || !self.refines_invariantly(tpe, sel) {
            return tpe;
        }
        let syms: Vec<SymbolId> = bound.into_iter().map(|(s, _)| s).collect();
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
        if !ok || bounds.iter().any(|&(lo, hi)| open(self, lo) || open(self, hi)) {
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
    /// arguments otherwise left unconstrained).
    fn refines_invariantly(&mut self, pat: TypeId, sel: TypeId) -> bool {
        let sel = self.dealiased(sel);
        let widened = self.u.widen(sel);
        if has_wildcard(self.u, widened) {
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
