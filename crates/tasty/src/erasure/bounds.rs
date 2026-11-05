// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Erasure of intersections (dotty's `erasedGlb`).

use super::{Erased, OBJECT};
use crate::tags::TRAIT;
use crate::types::{Type, TypeId};
use crate::{SymbolId, Universe};

/// How many value classes `erased_class` unwraps.
const MAX_UNWRAP: u32 = 16;

impl Universe {
    /// The erasure of `a & b`: of its `glb_side`.
    pub fn erase_and(&mut self, a: TypeId, b: TypeId) -> Erased {
        let side = self.glb_side(a, b);
        self.erase(side)
    }

    /// The side of `a & b` it erases as: the other side if one is `caps.Pure`, else the side first
    /// in dotty's `compareErasedGlb`.
    pub fn glb_side(&mut self, a: TypeId, b: TypeId) -> TypeId {
        let pure = |u: &mut Self, t| {
            let t = u.dealias(t);
            u.class_symbol(t).is_some_and(|c| u.full_name(c) == "scala.caps.Pure")
        };
        match pure(self, a) {
            true => b,
            false if pure(self, b) || self.glb_prefers(a, b) => a,
            false => b,
        }
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
        if let (Some(ca), Some(cb)) = (self.side_class(a, &ea), self.side_class(b, &eb)) {
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
            Erased::Class(_) => match self.side_class(t, e) {
                Some(s) if self.symbol(s).flags.has(TRAIT) => 3,
                _ => 2,
            },
            Erased::Nothing | Erased::Null => 2,
        }
    }

    /// The class `t` erases to if its erasure is `e`, else the class named by `e`.
    fn side_class(&mut self, t: TypeId, e: &Erased) -> Option<SymbolId> {
        let mut t = t;
        while let Type::This(p) | Type::Refined(p, ..) | Type::Annotated(p, _) = *self.types().get(t) {
            t = p;
        }
        let class = self.erased_class(t);
        let Erased::Class(name) = e else { return class };
        let own = class.filter(|_| name != OBJECT).map(|c| self.intern_type(Type::Sym(None, c)));
        if let Some(t) = own.filter(|&t| self.erase(t) == *e) {
            return self.class_symbol(t);
        }
        let r = self.class_ref(name);
        self.class_symbol(r).or(class)
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

    /// Whether `t` widens to a value class.
    pub fn is_value_class(&mut self, t: TypeId) -> bool {
        let t = self.widen(t);
        self.class_symbol(t).is_some_and(|c| self.underlying(c).is_some())
    }

    /// The element type of `Array[e]`.
    fn array_element(&mut self, t: TypeId) -> Option<TypeId> {
        let (tycon, arg) = self.applied(t).map(|(c, args)| (c, args.first().copied()))?;
        (self.class_name(tycon).as_deref() == Some("scala.Array")).then_some(arg)?
    }
}

fn name(e: &Erased) -> String {
    match e {
        Erased::Class(c) => c.clone(),
        other => format!("{other:?}"),
    }
}
