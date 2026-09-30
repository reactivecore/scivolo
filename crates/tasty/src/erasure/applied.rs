//! Erasure of applied types, arrays, unions and intersections.

use super::{Erased, OBJECT, UNBOUNDED};
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

impl Universe {
    /// `tycon[args]`: an array, a union or intersection written as `|[A, B]` or `&[A, B]`, else
    /// the erasure of `tycon`.
    pub fn erase_applied(&mut self, tycon: TypeId, args: &[TypeId]) -> Erased {
        match self.class_name(tycon).as_deref() {
            Some("scala.Array") if self.is_generic(args[0]) => Erased::Class(OBJECT.into()),
            Some("scala.Array") => {
                let elem = match self.erase(args[0]) {
                    Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
                    Erased::Nothing => Erased::Class("scala.runtime.Nothing$".into()),
                    Erased::Null => Erased::Class("scala.runtime.Null$".into()),
                    elem => elem,
                };
                Erased::Array(Box::new(elem))
            }
            Some("scala.|") if args.len() == 2 => self.erase_or(args[0], args[1]),
            Some("scala.&") if args.len() == 2 => self.erase_and(args[0], args[1]),
            _ => self.erase(tycon),
        }
    }

    /// Whether `t` is a wildcard or an abstract type bounded only by `Any` (or like it), which
    /// makes an array of it a generic array.
    fn is_generic(&mut self, t: TypeId) -> bool {
        match self.types.get(t).clone() {
            Type::Param(..) | Type::Local(..) => true,
            Type::Annotated(u) => self.is_generic(u),
            Type::Bounds(_, hi) => self.is_unbounded(hi),
            Type::TypeRef(p, n) => self.resolve(p, &n, true).is_some_and(|s| self.is_generic_symbol(s)),
            Type::Sym(_, s) => self.is_generic_symbol(s),
            _ => false,
        }
    }

    fn is_generic_symbol(&mut self, s: SymbolId) -> bool {
        if !matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) {
            return false;
        }
        let info = self.info(s);
        match self.types.get(info).clone() {
            Type::Bounds(_, hi) => self.is_unbounded(hi),
            Type::Lambda { .. } => false,
            _ => self.is_generic(info),
        }
    }

    /// Whether upper bound `hi` leaves an abstract type generic: `Any` and the like, or another
    /// generic type; not `AnyRef` or a class.
    fn is_unbounded(&mut self, hi: TypeId) -> bool {
        match self.class_name(hi) {
            Some(c) => UNBOUNDED.contains(&c.as_str()),
            None => match self.types.get(hi).clone() {
                Type::TypeRef(_, n) if n == "AnyRef" => false,
                _ => self.is_generic(hi),
            },
        }
    }

    /// The erasure of `a & b`: the non-`Object` side, else the left one.
    pub fn erase_and(&mut self, a: TypeId, b: TypeId) -> Erased {
        match (self.erase(a), self.erase(b)) {
            (Erased::Class(c), other) if c == OBJECT => other,
            (first, _) => first,
        }
    }

    /// The erasure of `a | b`: the side that is not `Null` or `Nothing`, else `Object` unless
    /// both erase alike.
    pub fn erase_or(&mut self, a: TypeId, b: TypeId) -> Erased {
        match (self.erase(a), self.erase(b)) {
            (Erased::Null | Erased::Nothing, other) | (other, Erased::Null | Erased::Nothing) => other,
            (x, y) if x == y => x,
            _ => Erased::Class(OBJECT.into()),
        }
    }
}
