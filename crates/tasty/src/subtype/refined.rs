// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Refinements on the right of a comparison (dotty's `hasMatchingMember`).

use super::Cmp;
use crate::types::{NameId, Type, TypeId};

impl Cmp<'_> {
    /// Whether `a` has a member `name` that conforms to refinement info `info`: a refinement of
    /// `a` itself, else a member of its class.
    pub fn has_matching_member(&mut self, a: TypeId, name: NameId, info: TypeId) -> bool {
        let t = self.u.widen(a);
        self.has_matching_member_in(a, t, name, info)
    }

    /// `has_matching_member` of `a` found in `t`, `a`'s widened type or a part of its
    /// intersection.
    fn has_matching_member_in(&mut self, a: TypeId, mut t: TypeId, name: NameId, info: TypeId) -> bool {
        let (mut recs, mut seen) = (Vec::new(), None);
        loop {
            match self.u.ty(t).clone() {
                Type::Refined(parent, n, have) if n == name => {
                    let have = self.u.rec_this_replaced(have, &recs, a);
                    let have = match seen {
                        Some(outer) => match self.bounds_and(outer, have) {
                            Some(both) => both,
                            None => return false,
                        },
                        None => have,
                    };
                    if self.info_conforms(have, info) {
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
                    return self.has_matching_member_in(a, l, name, info)
                        || self.has_matching_member_in(a, r, name, info);
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
        let name = self.u.name(name).to_owned();
        let members = self.u.member(class, &name);
        let site = match self.is_stable(a) {
            true => a,
            false => {
                let w = self.u.widen(a);
                if matches!(self.u.ty(w), Type::And(..) | Type::Refined(..)) { w } else { t }
            }
        };
        members.into_iter().any(|m| {
            let have = self.u.member_info(site, m);
            self.info_conforms(have, info)
                || self.abstract_member_matches(a, m, have, info)
                || self.stable_member_matches(a, m, info)
        })
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
        self.recur(selected, want)
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
        self.recur(lo2, selected) && self.recur(selected, hi2)
    }

    /// Whether member info `have` conforms to refinement info `want`: type bounds within
    /// bounds, a method type below a method type, a value's type below a value type.
    fn info_conforms(&mut self, have: TypeId, want: TypeId) -> bool {
        let bounds = |s: &mut Self, t: TypeId| match *s.u.ty(t) {
            Type::Bounds(lo, hi) => (lo, hi),
            Type::Alias(x) => (x, x),
            _ => (t, t),
        };
        match *self.u.ty(want) {
            Type::Bounds(..) | Type::Alias(_) => {
                let ((lo1, hi1), (lo2, hi2)) = (bounds(self, have), bounds(self, want));
                self.recur(lo2, lo1) && self.recur(hi1, hi2)
            }
            Type::Method { .. } | Type::Poly { .. } => self.recur(have, want),
            Type::ByName(want) => self.info_conforms(have, want),
            _ => match self.u.ty(have).clone() {
                Type::Method { params, result, .. } if self.u.params_of(params).is_empty() => {
                    self.recur(result, want)
                }
                Type::ByName(result) => self.recur(result, want),
                Type::Method { .. } | Type::Poly { .. } => false,
                _ => self.recur(have, want),
            },
        }
    }
}
