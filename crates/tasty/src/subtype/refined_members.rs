// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Class members against the refinement they are to implement (dotty's `qualifies` and `sigsOK`
//! of `hasMatchingMember`).

use super::Cmp;
use crate::tags::MUTABLE;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId};

/// The full name of the marker trait of `Selectable`s that dispatch without parameter types.
const WITHOUT_PRECISE_PARAMETER_TYPES: &str = "scala.Selectable.WithoutPreciseParameterTypes";

impl Cmp<'_> {
    /// Determines how info `have` of member `m` compares with refinement info `want`: `=> T` for a
    /// `def` without parameters, for a `var` and for a `val` against `{ def x: T }`.
    pub fn member_as_expr(&mut self, m: SymbolId, have: TypeId, want: TypeId) -> TypeId {
        if matches!(
            self.u.ty(have),
            Type::Method { .. }
                | Type::Poly { .. }
                | Type::ByName(_)
                | Type::Bounds(..)
                | Type::Alias(_)
                | Type::Lambda { .. }
        ) {
            return have;
        }
        let s = self.u.symbol(m);
        if !matches!(self.u.ty(want), Type::ByName(_)) && s.kind != Kind::Def && !s.flags.has(MUTABLE) {
            return have;
        }
        self.u.intern_type(Type::ByName(have))
    }

    /// Determines whether method `m`, of info `have`, may implement the method the last refinement
    /// of `refined` declares: in a bound or self type, if the refined type's class has a member of
    /// that name, so no reflection is needed, or dispatches without parameter types, or if `m` as
    /// declared has the parameter signature of the refinement.
    pub fn signatures_ok(&mut self, m: SymbolId, have: TypeId, refined: TypeId) -> bool {
        let Type::Refined(_, name, want) = *self.u.ty(refined) else { return true };
        let (Type::Method { .. }, Type::Method { .. }) = (*self.u.ty(have), *self.u.ty(want)) else {
            return true;
        };
        if self.u.checks_bounds_or_self_type() {
            return true;
        }
        if let Some(class) = self.refined_class(refined)
            && (!self.u.member(class, name).is_empty()
                || self
                    .u
                    .linearization(class)
                    .iter()
                    .any(|&b| self.u.full_name(b) == WITHOUT_PRECISE_PARAMETER_TYPES))
        {
            return true;
        }
        let mut declared = self.u.info(m);
        if let Type::Poly { result, .. } = *self.u.ty(declared) {
            declared = result;
        }
        let scala2 = self.u.erased_as_scala2(m);
        match (self.u.info_signature(declared, scala2), self.u.info_signature(want, false)) {
            (Some((p1, _)), Some((p2, _))) => p1 == p2 || self.has_type_var(want),
            _ => false,
        }
    }

    /// The class under the refinements of `t` (dotty's `underlyingClassRef(refinementOK = true)`).
    fn refined_class(&mut self, mut t: TypeId) -> Option<SymbolId> {
        while let Type::Refined(inner, ..) | Type::Rec(_, inner) | Type::Annotated(inner, _) = *self.u.ty(t) {
            t = inner;
        }
        self.u.class_symbol(t)
    }

    /// Whether `t` mentions a type still to be inferred, whose erasure is not known.
    fn has_type_var(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, x| {
            found |= u.var(x).is_some();
            None
        });
        found
    }
}
