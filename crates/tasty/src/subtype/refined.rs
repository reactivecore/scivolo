// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Refinements on the right of a comparison (dotty's `hasMatchingMember`).

use super::Cmp;
use crate::types::{Type, TypeId};

impl Cmp<'_> {
    /// Determines whether `a` has a member that conforms to the last refinement of `refined`: a
    /// refinement of `a` itself, else a member of its class under its own target name. A `def` or
    /// `var` does not conform to `{ val x: T }`, a `val`, `var` or `def x()` does to `{ def x: T }`.
    /// A method only the refinement declares must have its erased parameter types, except in
    /// bounds and self types (dotty's `hasMatchingMember`).
    pub fn has_matching_member(&mut self, a: TypeId, refined: TypeId) -> bool {
        let t = self.u.widen(a);
        self.has_matching_member_in(a, t, refined)
    }

    /// `has_matching_member` of `a` found in `t`, `a`'s widened type or a part of its
    /// intersection.
    fn has_matching_member_in(&mut self, a: TypeId, mut t: TypeId, refined: TypeId) -> bool {
        let Type::Refined(_, name, info) = *self.u.ty(refined) else { return false };
        let (mut recs, mut seen) = (Vec::new(), None);
        loop {
            match *self.u.ty(t) {
                Type::Refined(parent, n, have) if n == name => {
                    let have = self.u.rec_this_replaced(have, &recs, a);
                    let have = match seen {
                        Some(outer) => match self.bounds_and(outer, have) {
                            Some(both) => both,
                            None => return false,
                        },
                        None => have,
                    };
                    if self.sub_info(a, have, info) {
                        return true;
                    }
                    if self.u.bounds(have).is_none() {
                        return false;
                    }
                    (seen, t) = (Some(have), parent);
                }
                Type::Rec(b, parent) => {
                    recs.push(b);
                    t = parent;
                }
                Type::Refined(parent, ..) | Type::Annotated(parent, _) => t = parent,
                Type::And(l, r) if seen.is_none() => {
                    return self.has_matching_member_in(a, l, refined)
                        || self.has_matching_member_in(a, r, refined);
                }
                _ => match self.u.dealias(t) {
                    d if d != t && matches!(self.u.ty(d), Type::Rec(..) | Type::Refined(..)) => t = d,
                    _ => break,
                },
            }
        }
        if seen.is_some() {
            return false;
        }
        let Some(class) = self.u.class_symbol(t) else { return false };
        let members = self.u.member(class, name);
        let site = match self.is_stable(a) {
            true => a,
            false => {
                let w = self.u.widen(a);
                if matches!(self.u.ty(w), Type::And(..) | Type::Refined(..)) { w } else { t }
            }
        };
        members.into_iter().any(|m| {
            let target = self.u.symbol(m).target.as_ref();
            if target.is_some_and(|target| *target != self.u.symbol(m).name) {
                return false;
            }
            let have = self.u.member_info(site, m);
            let expr = self.member_as_expr(m, have, info);
            self.sub_info(a, expr, info) && self.signatures_ok(m, have, refined)
                || self.abstract_member_matches(a, m, have, info)
                || self.stable_member_matches(a, m, info)
        })
    }

    /// `info_conforms` for a member of `a`, the GADT constraint frozen if `a` is a singleton
    /// (dotty's `isSubInfo` in `hasMatchingMember`).
    fn sub_info(&mut self, a: TypeId, have: TypeId, want: TypeId) -> bool {
        match self.is_singleton(a) {
            true => self.in_frozen_gadt(|c| c.info_conforms(have, want)),
            false => self.info_conforms(have, want),
        }
    }

    /// The intersection of type bounds `x` and `y` (dotty's `TypeBounds.&`).
    fn bounds_and(&mut self, x: TypeId, y: TypeId) -> Option<TypeId> {
        let ((lo1, hi1), (lo2, hi2)) = (self.u.bounds(x)?, self.u.bounds(y)?);
        let (lo, hi) = (self.u.lub(lo1, lo2), self.u.glb(hi1, hi2));
        Some(self.u.intern_type(Type::Bounds(lo, hi)))
    }

    /// Whether `a.m`, for path `a` and stable value `m`, conforms to value type `want`.
    fn stable_member_matches(&mut self, a: TypeId, m: crate::SymbolId, want: TypeId) -> bool {
        let sym = self.u.symbol(m);
        let stable =
            matches!(sym.kind, crate::Kind::Val | crate::Kind::Param) && !sym.flags.has(crate::tags::MUTABLE);
        if !stable || !self.is_stable(a) || !self.is_value_type(want) {
            return false;
        }
        let selected = self.u.intern_type(Type::Sym(Some(a), m));
        self.recur_fresh(selected, want)
    }

    /// Whether abstract type member `m` of path `a`, selected on `a`, lies within the bounds
    /// `want` (dotty's `matchAbstractTypeMember`).
    fn abstract_member_matches(&mut self, a: TypeId, m: crate::SymbolId, have: TypeId, want: TypeId) -> bool {
        let abstract_member = self.u.bounds(have).is_some_and(|(lo, hi)| lo != hi);
        let (lo2, hi2) = match *self.u.ty(want) {
            Type::Bounds(lo, hi) => (lo, hi),
            Type::Alias(x) => (x, x),
            _ => return false,
        };
        if !abstract_member || !self.is_stable(a) {
            return false;
        }
        let a = self.u.by_name(a).unwrap_or(a);
        let selected = self.u.intern_type(Type::Sym(Some(a), m));
        self.recur_fresh(lo2, selected) && self.recur_fresh(selected, hi2)
    }

    /// Whether member info `have` conforms to refinement info `want`: type bounds within
    /// bounds, a method type below a method type, `=> T`, `(): T` or a value's type below `=> U`, a
    /// value's type below a value type.
    fn info_conforms(&mut self, have: TypeId, want: TypeId) -> bool {
        let bounds = |s: &mut Self, t: TypeId| match *s.u.ty(t) {
            Type::Bounds(lo, hi) => (lo, hi),
            Type::Alias(x) => (x, x),
            _ => (t, t),
        };
        match (*self.u.ty(have), *self.u.ty(want)) {
            (_, Type::Bounds(..) | Type::Alias(_)) => {
                let ((lo1, hi1), (lo2, hi2)) = (bounds(self, have), bounds(self, want));
                self.recur_fresh(lo2, lo1) && self.recur_fresh(hi1, hi2)
            }
            (_, Type::Method { .. } | Type::Poly { .. }) => self.recur_fresh(have, want),
            (Type::Method { params, result, .. }, Type::ByName(want))
                if self.u.params_of(params).is_empty() =>
            {
                self.recur_fresh(result, want)
            }
            (Type::Method { .. } | Type::Poly { .. }, _) => false,
            (Type::ByName(have), Type::ByName(want)) => self.recur_fresh(have, want),
            (Type::ByName(_), _) => false,
            (_, Type::ByName(want)) => self.recur_fresh(have, want),
            _ => self.recur_fresh(have, want),
        }
    }
}
