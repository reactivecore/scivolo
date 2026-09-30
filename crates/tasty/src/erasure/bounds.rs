//! Erasure of intersections and unions (dotty's `erasedGlb` and `erasedLub`, without subclass
//! relations, which need class parents).

use super::{Erased, OBJECT, object};
use crate::Universe;
use crate::tags::TRAIT;
use crate::types::TypeId;

impl Universe {
    /// The erasure of `a & b`: of the two erasures, the first in dotty's order (arrays,
    /// primitives, classes, traits, then by name), `Object` last.
    pub fn erase_and(&mut self, a: TypeId, b: TypeId) -> Erased {
        let (ea, eb) = (self.erase(a), self.erase(b));
        let (ra, rb) = (self.glb_rank(a, &ea), self.glb_rank(b, &eb));
        if (ra, name(&ea)) <= (rb, name(&eb)) { ea } else { eb }
    }

    /// The rank of type `t` with erasure `e` in dotty's order for intersections.
    fn glb_rank(&mut self, t: TypeId, e: &Erased) -> u8 {
        match e {
            Erased::Array(_) => 0,
            Erased::Prim(_) | Erased::Unit => 1,
            Erased::Class(c) if c == OBJECT => 4,
            Erased::Class(_) => match self.class_symbol(t) {
                Some(s) if self.symbol(s).flags.has(TRAIT) => 3,
                _ => 2,
            },
            Erased::Nothing | Erased::Null => 2,
        }
    }

    /// The erasure of `a | b`: the other side for `Nothing`, and for `Null` beside a reference;
    /// one side if both erase alike; an array of the joined elements; else `Object`.
    pub fn erase_or(&mut self, a: TypeId, b: TypeId) -> Erased {
        let (ea, eb) = (self.erase(a), self.erase(b));
        lub(ea, eb)
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
