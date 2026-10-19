// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Erasure of intersections and unions (dotty's `erasedGlb` and `erasedLub`).

use super::{Erased, OBJECT, object};
use crate::tags::TRAIT;
use crate::types::{Type, TypeId};
use crate::{SymbolId, Universe};

/// How many value classes `erased_class` unwraps.
const MAX_UNWRAP: u32 = 16;

impl Universe {
    /// The erasure of `a & b`: of the other side if one is `caps.Pure`, else of the side first in
    /// dotty's `compareErasedGlb`.
    pub fn erase_and(&mut self, a: TypeId, b: TypeId) -> Erased {
        let pure = |u: &mut Self, t| {
            let t = u.dealias(t);
            u.class_symbol(t).is_some_and(|c| u.full_name(c) == "scala.caps.Pure")
        };
        let first = if pure(self, a) {
            b
        } else if pure(self, b) || self.glb_prefers(a, b) {
            a
        } else {
            b
        };
        self.erase(first)
    }

    /// Whether `a` comes before `b` in dotty's order for intersections: value classes, arrays
    /// (by element), primitives, classes, traits, `Object`; subclasses before their bases, then
    /// by name.
    fn glb_prefers(&mut self, a: TypeId, b: TypeId) -> bool {
        let (ea, eb) = (self.erase(a), self.erase(b));
        if ea == eb {
            return true;
        }
        let (va, vb) = (self.is_value_class(a), self.is_value_class(b));
        if va || vb {
            return va;
        }
        if let (Some(x), Some(y)) = (self.array_element(a), self.array_element(b)) {
            return self.glb_prefers(x, y);
        }
        let (ra, rb) = (self.glb_rank(a, &ea), self.glb_rank(b, &eb));
        if ra != rb {
            return ra < rb;
        }
        if let (Some(ca), Some(cb)) = (self.erased_class(a), self.erased_class(b)) {
            if self.derives_from(ca, cb) {
                return true;
            }
            if self.derives_from(cb, ca) {
                return false;
            }
        }
        name(&ea) <= name(&eb)
    }

    fn glb_rank(&mut self, t: TypeId, e: &Erased) -> u8 {
        match e {
            Erased::Array(_) => 0,
            Erased::Prim(_) | Erased::Unit | Erased::Value(..) => 1,
            Erased::Class(c) if c == OBJECT => 2,
            Erased::Class(name) => {
                let class = self.erased_class(t).or_else(|| {
                    let r = self.class_ref(name);
                    self.class_symbol(r)
                });
                match class {
                    Some(s) if self.symbol(s).flags.has(TRAIT) => 3,
                    _ => 2,
                }
            }
            Erased::Nothing | Erased::Null => 2,
        }
    }

    /// The erasure of `a | b`: the other side for `Nothing`; `Object` beside a value class; the
    /// other side for `Null` beside a reference; one side if both erase alike; an array of the
    /// joined elements; the closest common base class of the erased sides; else `Object`.
    pub fn erase_or(&mut self, a: TypeId, b: TypeId) -> Erased {
        let (ea, eb) = (self.erase(a), self.erase(b));
        let nothing = ea == Erased::Nothing || eb == Erased::Nothing;
        if !nothing && (self.is_value_class(a) || self.is_value_class(b)) {
            return object();
        }
        if let (Erased::Class(na), Erased::Class(nb)) = (&ea, &eb)
            && ea != eb
        {
            let class = |u: &mut Self, t: TypeId, name: &str| {
                let widened = u.widen(t);
                u.erased_class(widened).or_else(|| {
                    let r = u.class_ref(name);
                    u.class_symbol(r)
                })
            };
            let (Some(ca), Some(cb)) = (class(self, a, na), class(self, b, nb)) else {
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
        let mut minimal = Vec::new();
        for &c in &candidates {
            if self.full_name(c) != "scala.*:"
                && candidates.iter().all(|&x| x == c || !self.derives_from(x, c))
            {
                minimal.push(c);
            }
        }
        let base = *minimal.last()?;
        let full = self.full_name(base);
        (!matches!(full.as_str(), "scala.Any" | "scala.AnyVal")).then_some(base)
    }

    /// The class `t` erases to: its class, a value class unwrapped to its underlying type's.
    pub fn erased_class(&mut self, t: TypeId) -> Option<SymbolId> {
        let mut c = self.class_symbol(t)?;
        for _ in 0..MAX_UNWRAP {
            let Some(value) = self.underlying(c) else { return Some(c) };
            let info = self.info(value);
            c = self.class_symbol(info)?;
        }
        None
    }

    fn is_value_class(&mut self, t: TypeId) -> bool {
        let t = self.widen(t);
        self.class_symbol(t).is_some_and(|c| self.underlying(c).is_some())
    }

    /// The element type of `Array[e]`.
    fn array_element(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Applied(tycon, args) = self.types().get(t).clone() else { return None };
        (self.class_name(tycon).as_deref() == Some("scala.Array")).then(|| args.first().copied())?
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
            (x @ Erased::Prim(_), y) | (y, x @ Erased::Prim(_)) => {
                if x == y {
                    Erased::Array(Box::new(x))
                } else {
                    object()
                }
            }
            (x, y) => Erased::Array(Box::new(lub(x, y))),
        },
        _ => object(),
    }
}

fn name(e: &Erased) -> String {
    match e {
        Erased::Class(c) => c.clone(),
        other => format!("{other:?}"),
    }
}
