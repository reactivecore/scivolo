//! Which types have `null` among their values (dotty's `isNullable`, without explicit nulls).

use super::Cmp;
use super::refs::Meaning;
use crate::SymbolId;
use crate::tags::{NULLconst, OBJECT};
use crate::types::{Type, TypeId};

/// How many singletons `nullable` looks through.
const MAX_NULLABLE: u32 = 64;

impl Cmp<'_> {
    /// Whether `null` is a value of `t`.
    pub fn nullable(&mut self, t: TypeId) -> bool {
        self.nullable_within(t, MAX_NULLABLE)
    }

    fn nullable_within(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.u.dealias(t);
        let widened = |s: &mut Self| {
            let w = s.u.widen(t);
            w != t && s.nullable_within(w, depth)
        };
        match self.u.types().get(t).clone() {
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(t).map(|n| n.meaning) {
                Some(Meaning::Class(c)) => self.nullable_class(c),
                Some(Meaning::Term(_)) => widened(self),
                _ => false,
            },
            Type::This(_) => widened(self),
            Type::Refined(p, ..) | Type::Rec(_, p) | Type::Annotated(p, _) | Type::Applied(p, _) => {
                self.nullable_within(p, depth)
            }
            Type::And(a, b) => self.nullable_within(a, depth) && self.nullable_within(b, depth),
            Type::Or(a, b, _) => self.nullable_within(a, depth) || self.nullable_within(b, depth),
            Type::Constant(tag, _) => tag == NULLconst,
            _ => false,
        }
    }

    /// Whether class `c` has `null` among its values: not `Nothing`, a value class or an object.
    fn nullable_class(&mut self, c: SymbolId) -> bool {
        let builtins = self.u.builtins();
        c != builtins.nothing
            && !self.u.symbol(c).flags.has(OBJECT)
            && !builtins.any_val.is_some_and(|v| self.u.derives_from(c, v))
    }
}
