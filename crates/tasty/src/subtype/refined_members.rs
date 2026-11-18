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

/// The packages of the classes the universe may hold as Scala definitions of Java ones.
const JAVA_PACKAGES: [&str; 2] = ["java.", "javax."];

impl Cmp<'_> {
    /// Determines how info `have` of member `m` of `class` compares with refinement info `want`:
    /// `=> T` for a `def` without parameters, for a `var`, for a `val` against `{ def x: T }` and
    /// for a Java `def x(): T` against it.
    pub fn member_as_expr(&mut self, (m, class): (SymbolId, SymbolId), have: TypeId, want: TypeId) -> TypeId {
        let by_name = matches!(self.u.ty(want), Type::ByName(_));
        let info = *self.u.ty(have);
        let result = match info {
            Type::Method { params, result, .. }
                if by_name && self.u.params_of(params).is_empty() && self.is_java_member(m, class) =>
            {
                result
            }
            Type::Method { .. }
            | Type::Poly { .. }
            | Type::ByName(_)
            | Type::Bounds(..)
            | Type::Alias(_)
            | Type::Lambda { .. } => return have,
            _ => {
                let s = self.u.symbol(m);
                if !by_name && s.kind != Kind::Def && !s.flags.has(MUTABLE) {
                    return have;
                }
                have
            }
        };
        self.u.intern_type(Type::ByName(result))
    }

    /// Determines whether method `m`, of info `have`, may implement the method the last refinement
    /// of `refined` declares: the refined type's class has a member of that name, so no reflection
    /// is needed, or dispatches without parameter types, or `m` as declared has the parameter
    /// signature of the refinement.
    pub fn signatures_ok(&mut self, m: SymbolId, have: TypeId, refined: TypeId) -> bool {
        let Type::Refined(_, name, want) = *self.u.ty(refined) else { return true };
        let (Type::Method { .. }, Type::Method { .. }) = (*self.u.ty(have), *self.u.ty(want)) else {
            return true;
        };
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

    /// Whether `m`, a member of `class`, is defined by Java: from a class file, a member of a JDK
    /// class the universe holds as a Scala definition, or a member of `Any` that `Object` declares
    /// to dotty.
    fn is_java_member(&mut self, m: SymbolId, class: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(m).owner else { return false };
        let any = self.u.defn().any;
        owner == any && class != any
            || self.u.is_java_defined(m)
            || self.u.may_be_java(owner)
                && JAVA_PACKAGES.iter().any(|p| self.u.full_name_starts_with(owner, p))
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
