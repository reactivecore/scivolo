//! Erasure of intersections and unions (dotty's `erasedGlb` and `erasedLub`).

use super::{Erased, OBJECT, object};
use crate::tags::TRAIT;
use crate::types::TypeId;
use crate::{SymbolId, Universe};

impl Universe {
    /// The erasure of `a & b`: of the two erasures, the first in dotty's order (arrays,
    /// primitives, classes, traits, subclasses before their base classes, then by name),
    /// `Object` last.
    pub fn erase_and(&mut self, a: TypeId, b: TypeId) -> Erased {
        let (ea, eb) = (self.erase(a), self.erase(b));
        let (ra, rb) = (self.glb_rank(a, &ea), self.glb_rank(b, &eb));
        if ra != rb {
            return if ra < rb { ea } else { eb };
        }
        if let (Some(ca), Some(cb)) = (self.class_symbol(a), self.class_symbol(b)) {
            if self.derives_from(ca, cb) {
                return ea;
            }
            if self.derives_from(cb, ca) {
                return eb;
            }
        }
        if name(&ea) <= name(&eb) { ea } else { eb }
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
    /// one side if both erase alike; an array of the joined elements; the closest common base
    /// class; else `Object`.
    pub fn erase_or(&mut self, a: TypeId, b: TypeId) -> Erased {
        let (ea, eb) = (self.erase(a), self.erase(b));
        if let (Erased::Class(_), Erased::Class(_)) = (&ea, &eb)
            && ea != eb
            && let (Some(ca), Some(cb)) = (self.class_symbol(a), self.class_symbol(b))
        {
            return match self.common_base(ca, cb) {
                Some(base) => {
                    let base = self.intern_type(crate::Type::Sym(None, base));
                    self.erase(base)
                }
                None => object(),
            };
        }
        lub(ea, eb)
    }

    /// The closest base class of `a` that `b` derives from, a class rather than a trait if
    /// there is one (dotty's `erasedLub` loop).
    fn common_base(&mut self, a: SymbolId, b: SymbolId) -> Option<SymbolId> {
        let mut best: Option<SymbolId> = None;
        for base in self.linearization(a) {
            if !self.derives_from(b, base) {
                continue;
            }
            let better = match best {
                Some(prev) if self.derives_from(prev, base) => prev,
                _ => base,
            };
            best = Some(better);
            if !self.symbol(base).flags.has(TRAIT) {
                return best;
            }
        }
        best
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
