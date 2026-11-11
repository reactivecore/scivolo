// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Erasure of unions (dotty's `erasedLub`).

use super::{Erased, object};
use crate::tags::TRAIT;
use crate::types::{Type, TypeId};
use crate::{SymbolId, Universe};

impl Universe {
    /// The erasure of `a | b`: the other side for `Nothing`; `Object` beside a value class; the
    /// other side for `Null` beside a reference, `BoxedUnit` beside `Unit`; one side if both erase
    /// alike; an array of the joined elements; the closest common base class of the erased sides;
    /// else `Object`.
    pub fn erase_or(&mut self, a: TypeId, b: TypeId) -> Erased {
        let (ea, eb) = (self.erase(a), self.erase(b));
        if matches!((&ea, &eb), (Erased::Unit, Erased::Null) | (Erased::Null, Erased::Unit)) {
            return Erased::boxed_unit();
        }
        let nothing = ea == Erased::Nothing || eb == Erased::Nothing;
        if !nothing && (self.is_value_class(a) || self.is_value_class(b)) {
            return object();
        }
        if let (Erased::Class(na), Erased::Class(nb)) = (&ea, &eb)
            && ea != eb
        {
            let (Some(ca), Some(cb)) = (self.lub_class(a, na), self.lub_class(b, nb)) else {
                return object();
            };
            return match self.common_base(ca, cb) {
                Some(base) => {
                    let base = self.intern_type(Type::Sym(None, base));
                    self.erase(base)
                }
                None => object(),
            };
        }
        lub(ea, eb)
    }

    /// The class of side `t` of a union, erased to class `name`: of the side `glb_side` picks of an
    /// intersection, the common base of the sides of a union without a class, else the class
    /// named `name`.
    fn lub_class(&mut self, t: TypeId, name: &str) -> Option<SymbolId> {
        let mut widened = self.widen(t);
        while let Type::And(x, y) = *self.ty(widened) {
            widened = self.glb_side(x, y);
        }
        let erases_to = |u: &mut Self, c: SymbolId, name: &str| {
            let t = u.intern_type(Type::Sym(None, c));
            matches!(u.erase(t), Erased::Class(n) if n == name)
        };
        let side = |u: &mut Self, s: TypeId| match u.erase(s) {
            Erased::Class(n) => u.lub_class(s, &n).filter(|&c| erases_to(u, c, &n)),
            _ => None,
        };
        let named = |u: &mut Self, c: SymbolId| erases_to(u, c, name);
        let found = match (self.erased_class(widened), *self.ty(widened)) {
            (None, Type::Or(x, y, _)) => match (side(self, x), side(self, y)) {
                (Some(cx), Some(cy)) => self.common_base(cx, cy),
                (cx, cy) => cx.or(cy),
            }
            .filter(|&c| named(self, c)),
            (found, _) => found,
        };
        found.or_else(|| {
            let r = self.class_ref(name);
            self.class_symbol(r)
        })
    }

    /// The common base of `a` and `b` in dotty's `erasedLub`: of the bases of `a` but `Pure` that
    /// `b` derives from, up to and including the first trait, the last one no other of them
    /// derives from.
    fn common_base(&mut self, a: SymbolId, b: SymbolId) -> Option<SymbolId> {
        let mut candidates = Vec::new();
        for base in self.linearization(a) {
            if self.full_name(base) != "scala.caps.Pure" && self.derives_from(b, base) {
                candidates.push(base);
                if self.symbol(base).flags.has(TRAIT) {
                    break;
                }
            }
        }
        let minimal = |u: &mut Self, c: SymbolId| {
            u.full_name(c) != "scala.*:" && candidates.iter().all(|&x| x == c || !u.derives_from(x, c))
        };
        let base = candidates.iter().copied().rfind(|&c| minimal(self, c))?;
        let full = self.full_name(base);
        (!matches!(full.as_str(), "scala.Any" | "scala.AnyVal")).then_some(base)
    }
}

fn is_reference(e: &Erased) -> bool {
    matches!(e, Erased::Class(_) | Erased::Array(_) | Erased::Null | Erased::Nothing)
}

fn lub(a: Erased, b: Erased) -> Erased {
    match (a, b) {
        (Erased::Nothing, x) | (x, Erased::Nothing) => x,
        (Erased::Null, x) | (x, Erased::Null) if is_reference(&x) => x,
        (x, y) if x == y => x,
        (Erased::Array(x), Erased::Array(y)) => match (*x, *y) {
            (Erased::Prim(_), _) | (_, Erased::Prim(_)) => object(),
            (x, y) => Erased::Array(Box::new(lub(x, y))),
        },
        _ => object(),
    }
}
