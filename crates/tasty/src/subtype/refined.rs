// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Refinements on the right of a comparison (dotty's `hasMatchingMember`).

use super::Cmp;
use crate::types::{Type, TypeId};

impl Cmp<'_> {
    /// Whether `a` has a member `name` that conforms to refinement info `info`: a refinement of
    /// `a` itself, else a member of its class.
    pub fn has_matching_member(&mut self, a: TypeId, name: &str, info: TypeId) -> bool {
        let mut t = self.u.widen(a);
        loop {
            match self.u.types().get(t).clone() {
                Type::Refined(_, n, have) if n == name => return self.info_conforms(have, info),
                Type::Refined(parent, ..) | Type::Annotated(parent, _) | Type::Rec(_, parent) => t = parent,
                _ => break,
            }
        }
        let Some(class) = self.u.class_symbol(t) else { return false };
        let members = self.u.member(class, name);
        members.into_iter().any(|m| {
            let have = self.u.member_info(t, m);
            self.info_conforms(have, info)
        })
    }

    /// Whether member info `have` conforms to refinement info `want`: type bounds within
    /// bounds, a method type below a method type, a value's type below a value type.
    fn info_conforms(&mut self, have: TypeId, want: TypeId) -> bool {
        let bounds = |s: &mut Self, t: TypeId| match s.u.types().get(t).clone() {
            Type::Bounds(lo, hi) => (lo, hi),
            Type::Alias(x) => (x, x),
            _ => (t, t),
        };
        match self.u.types().get(want).clone() {
            Type::Bounds(..) | Type::Alias(_) => {
                let ((lo1, hi1), (lo2, hi2)) = (bounds(self, have), bounds(self, want));
                self.recur(lo2, lo1) && self.recur(hi1, hi2)
            }
            Type::Method { .. } | Type::Poly { .. } => self.recur(have, want),
            Type::ByName(want) => self.info_conforms(have, want),
            _ => match self.u.types().get(have).clone() {
                Type::Method { params, result, .. } if params.is_empty() => self.recur(result, want),
                Type::ByName(result) => self.recur(result, want),
                Type::Method { .. } | Type::Poly { .. } => false,
                _ => self.recur(have, want),
            },
        }
    }
}
