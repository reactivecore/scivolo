// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Least upper bounds of ground types (dotty's `lub`, without `orDominator`: a union stays a
//! union, soft unless asked otherwise).

use super::hk::Connective;
use super::{Cmp, NoVars, TypeVars};
use crate::tags::{INLINE, MUTABLE, STABLE};
use crate::types::{Type, TypeId};
use crate::{ANY, Kind, SymbolId, Universe};

impl Universe {
    /// The least upper bound of `a` and `b`: one of them if it contains the other, else their
    /// soft union with the parts the other covers dropped.
    pub fn lub(&mut self, a: TypeId, b: TypeId) -> TypeId {
        Cmp::new(self, &mut NoVars).lub(a, b)
    }

    /// `lub` under the constraint `vars`, which it only consults.
    pub fn lub_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> TypeId {
        let mut c = Cmp::new(self, vars);
        c.frozen = true;
        c.lub(a, b)
    }

    /// `lub`, a union soft if `soft`.
    pub fn lub_as(&mut self, a: TypeId, b: TypeId, soft: bool) -> TypeId {
        Cmp::new(self, &mut NoVars).lub_as(a, b, soft)
    }
}

impl Cmp<'_> {
    pub fn lub(&mut self, a: TypeId, b: TypeId) -> TypeId {
        self.lub_as(a, b, true)
    }

    pub fn lub_as(&mut self, a: TypeId, b: TypeId, soft: bool) -> TypeId {
        if a == b || self.is_top(a, b) || self.is_bottom(b) {
            return a;
        }
        if self.is_top(b, a) || self.is_bottom(a) {
            return b;
        }
        if self.is_stable(a) && self.recur_frozen(a, b) {
            return b;
        }
        if self.is_stable(b) && self.recur_frozen(b, a) {
            return a;
        }
        if let Some(joined) = self.atoms_lub(a, b, soft) {
            return joined;
        }
        let (a, b) = (self.widen_if_unstable(a), self.widen_if_unstable(b));
        let b_rest = self.drop_if_sub(b, a);
        let a_rest = match b_rest {
            Some(b) => self.drop_if_sub(a, b),
            None => Some(a),
        };
        match (a_rest, b_rest) {
            (Some(x), Some(y)) => self.or_type(x, y, soft),
            (Some(x), None) | (None, Some(x)) => x,
            (None, None) => a,
        }
    }

    /// `t` widened if it refers to a variable, method or by-name parameter (dotty's
    /// `widenIfUnstable`).
    fn widen_if_unstable(&mut self, t: TypeId) -> TypeId {
        let Type::Sym(_, s) = *self.u.types().get(t) else { return t };
        let widened = if self.is_unstable(s) { self.u.widen(t) } else { return t };
        match *self.u.types().get(widened) {
            Type::ByName(r) => r,
            _ => widened,
        }
    }

    /// Whether term `s` is a variable, a method that is not stable, or a by-name or inline
    /// parameter.
    pub fn is_unstable(&mut self, s: SymbolId) -> bool {
        let (kind, flags, info) = (self.u.symbol(s).kind, self.u.symbol(s).flags, self.u.info(s));
        match kind {
            Kind::Val => flags.has(MUTABLE),
            Kind::Def => !flags.has(STABLE),
            Kind::Param => flags.has(INLINE) || matches!(self.u.types().get(info), Type::ByName(_)),
            _ => false,
        }
    }

    /// Whether `top` is `Any` for a type `other` that is not a constructor, or `AnyKind`.
    pub fn is_top(&mut self, top: TypeId, other: TypeId) -> bool {
        let any_kind = self.u.intern_type(Type::Sym(None, self.u.builtins().any_kind));
        top == ANY && self.constructor_arity(other).is_none() || top == any_kind
    }

    /// `t` without its `|`-parts that are subtypes of `sup`; `None` if none remains.
    fn drop_if_sub(&mut self, t: TypeId, sup: TypeId) -> Option<TypeId> {
        match *self.u.types().get(t) {
            Type::Or(x, y, soft) => match (self.drop_if_sub(x, sup), self.drop_if_sub(y, sup)) {
                (Some(x2), Some(y2)) if (x2, y2) == (x, y) => Some(t),
                (Some(x2), Some(y2)) => Some(self.u.intern_type(Type::Or(x2, y2, soft))),
                (Some(r), None) | (None, Some(r)) => Some(r),
                (None, None) => None,
            },
            _ => (!self.recur_frozen(t, sup)).then_some(t),
        }
    }

    /// `a | b`, soft if `soft`, by-name results joined.
    pub fn or_type(&mut self, a: TypeId, b: TypeId, soft: bool) -> TypeId {
        match (self.u.types().get(a), self.u.types().get(b)) {
            (&Type::Annotated(x, _), _) => self.lub_as(x, b, soft),
            (_, &Type::Annotated(y, _)) => self.lub_as(a, y, soft),
            (&Type::ByName(x), &Type::ByName(y)) => {
                let either = self.lub_as(x, y, soft);
                self.u.intern_type(Type::ByName(either))
            }
            _ => self.lift_if_constructors(a, b, Connective::Or(soft)),
        }
    }
}
