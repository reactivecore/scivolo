//! Least upper bounds of ground types (dotty's `lub`, without `orDominator`: a union stays a
//! union).

use super::{Cmp, NoVars};
use crate::tags::{INLINE, MUTABLE, STABLE};
use crate::types::{Type, TypeId};
use crate::{ANY, Kind, SymbolId, Universe};

impl Universe {
    /// The least upper bound of `a` and `b`: one of them if it contains the other, else their
    /// union with the parts the other covers dropped.
    pub fn lub(&mut self, a: TypeId, b: TypeId) -> TypeId {
        Cmp::new(self, &mut NoVars).lub(a, b)
    }
}

impl Cmp<'_> {
    pub fn lub(&mut self, a: TypeId, b: TypeId) -> TypeId {
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
        if let Some(joined) = self.atoms_lub(a, b) {
            return joined;
        }
        let (a, b) = (self.widen_if_unstable(a), self.widen_if_unstable(b));
        let b_rest = self.drop_if_sub(b, a);
        let a_rest = match b_rest {
            Some(b) => self.drop_if_sub(a, b),
            None => Some(a),
        };
        match (a_rest, b_rest) {
            (Some(x), Some(y)) => self.or_type(x, y),
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
        match self.u.types().get(t).clone() {
            Type::Or(x, y) => match (self.drop_if_sub(x, sup), self.drop_if_sub(y, sup)) {
                (Some(x2), Some(y2)) if (x2, y2) == (x, y) => Some(t),
                (Some(x2), Some(y2)) => Some(self.u.intern_type(Type::Or(x2, y2))),
                (Some(r), None) | (None, Some(r)) => Some(r),
                (None, None) => None,
            },
            _ => (!self.recur_frozen(t, sup)).then_some(t),
        }
    }

    /// `a | b`, by-name results joined.
    pub fn or_type(&mut self, a: TypeId, b: TypeId) -> TypeId {
        match (self.u.types().get(a).clone(), self.u.types().get(b).clone()) {
            (Type::Annotated(x, _), _) => self.lub(x, b),
            (_, Type::Annotated(y, _)) => self.lub(a, y),
            (Type::ByName(x), Type::ByName(y)) => {
                let either = self.lub(x, y);
                self.u.intern_type(Type::ByName(either))
            }
            _ => self.lift_if_constructors(a, b, false),
        }
    }
}
