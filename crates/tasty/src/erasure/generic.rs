// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Generic array elements (dotty's `isGenericArrayElement`): types no JVM array holds all values
//! of, whose arrays erase to `Object`.

use super::Erased;
use super::special::{UNBOUNDED, special};
use crate::types::{Type, TypeId};
use crate::{Kind, Universe};

/// The kind of JVM array that can hold every value of a type (dotty's `arrayUpperBound`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Holder {
    None,
    Object,
    Prim(char),
}

impl Universe {
    /// Whether an array of `t` erases to `Object`: no JVM array holds all its values.
    pub fn is_generic(&mut self, t: TypeId) -> bool {
        match self.types().get(t).clone() {
            Type::Param(..) | Type::Local(..) => true,
            Type::Bounds(_, hi) => self.holder(hi) == Holder::None,
            Type::Annotated(u, _)
            | Type::Flexible(u)
            | Type::Refined(u, _, _)
            | Type::Rec(_, u)
            | Type::Alias(u) => self.is_generic(u),
            Type::And(a, b) => self.is_generic(a) && self.is_generic(b),
            Type::Or(a, b, _) => self.is_generic(a) || self.is_generic(b),
            Type::Match { cases, .. } => {
                let mut holders = cases.into_iter().map(|c| match self.case_body(c) {
                    Some(body) => self.holder(body),
                    None => Holder::None,
                });
                let first = holders.next();
                first.is_some_and(|h| h == Holder::None || holders.any(|x| x != h))
            }
            Type::Applied(tycon, args) => match self.expand(tycon, &args) {
                Some(alias) => self.is_generic(alias),
                None if let Some(alias) = self.overriding_member(tycon) => {
                    let applied = self.intern_type(Type::Applied(alias, args));
                    let reduced = self.dealias_step(applied);
                    self.is_generic(reduced)
                }
                None if self.is_abstract(tycon) => match self.expand_upper(tycon, &args) {
                    Some(upper) => self.holder(upper) == Holder::None,
                    None => true,
                },
                None => false,
            },
            Type::Sym(..) | Type::TypeRef(..) => match self.referenced(t) {
                _ if let Some(alias) = self.overriding_member(t) => self.is_generic(alias),
                Some(s) if matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) => {
                    let info = self.opaque_alias(s).unwrap_or_else(|| self.info(s));
                    match *self.types().get(info) {
                        Type::Bounds(_, hi) => self.holder(hi) == Holder::None,
                        Type::Lambda { .. } => false,
                        _ => self.is_generic(info),
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// The alias or class by which the class of the prefix of abstract type reference `t`
    /// overrides it (dotty's `dealias` of a member found through the prefix).
    fn overriding_member(&mut self, t: TypeId) -> Option<TypeId> {
        let prefix = match *self.types().get(t) {
            Type::Sym(p, _) => p?,
            Type::TypeRef(p, _) => p,
            _ => return None,
        };
        let s = self.referenced(t)?;
        self.overriding_alias(prefix, s)
    }

    /// The body of match type case `case`, under its captures' lambda if any.
    fn case_body(&self, case: TypeId) -> Option<TypeId> {
        let case = match *self.types().get(case) {
            Type::Lambda { result, .. } => result,
            _ => case,
        };
        match *self.types().get(case) {
            Type::MatchCase(_, body) => Some(body),
            _ => None,
        }
    }

    /// Whether `tycon` is an abstract type or type parameter, not a class or alias.
    fn is_abstract(&mut self, tycon: TypeId) -> bool {
        let Some(s) = self.referenced(tycon) else { return false };
        let info = self.info(s);
        matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam)
            && matches!(self.types().get(info), Type::Bounds(..))
    }

    /// The kind of array that can hold every value of type `t`.
    fn holder(&mut self, t: TypeId) -> Holder {
        match *self.types().get(t) {
            Type::Annotated(u, _)
            | Type::Flexible(u)
            | Type::Refined(u, _, _)
            | Type::Rec(_, u)
            | Type::Alias(u)
            | Type::Bounds(_, u) => self.holder(u),
            Type::Applied(c, _) => {
                let args = self.applied(t).map(|(_, a)| a.to_vec()).unwrap_or_default();
                match self.expand(c, &args).or_else(|| self.expand_opaque(c, &args)) {
                    Some(expanded) => self.holder(expanded),
                    None => self.holder(c),
                }
            }
            Type::And(a, b) => match (self.holder(a), self.holder(b)) {
                (x, y) if x == y => x,
                (Holder::None, y) => y,
                (x, _) => x,
            },
            Type::Or(a, b, _) => match (self.holder(a), self.holder(b)) {
                (x, y) if x == y => x,
                _ => Holder::None,
            },
            Type::Sym(..) | Type::TypeRef(..) => {
                if let Some(c) = self.class_name(t) {
                    return match special(&c) {
                        _ if UNBOUNDED.contains(&c.as_str()) => Holder::None,
                        Some(Erased::Prim(p)) => Holder::Prim(p),
                        _ => Holder::Object,
                    };
                }
                let Some(s) = self.referenced(t) else { return Holder::None };
                let info = self.opaque_alias(s).unwrap_or_else(|| self.info(s));
                if matches!(self.types().get(info), Type::Lambda { .. }) {
                    Holder::None
                } else {
                    self.holder(info)
                }
            }
            _ => Holder::None,
        }
    }
}
