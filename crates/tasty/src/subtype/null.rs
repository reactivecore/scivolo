// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Which types have `null` among their values (dotty's `isNullable`, `isNullableClass`).

use super::Cmp;
use super::refs::Meaning;
use crate::SymbolId;
use crate::tags::{NULLconst, OBJECT};
use crate::types::{Type, TypeId};

/// How comparisons treat `Null`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct NullMode {
    /// `-Yexplicit-nulls` (dotty's `ctx.explicitNulls`).
    pub explicit: bool,
    /// Explicit nulls without `unsafeNulls` (dotty's `Mode.SafeNulls`).
    pub safe: bool,
}

/// How many singletons `nullable` looks through.
const MAX_NULLABLE: u32 = 64;

impl Cmp<'_> {
    pub fn explicit_nulls(&self) -> bool {
        self.u.null_mode().explicit
    }

    pub fn safe_nulls(&self) -> bool {
        self.u.null_mode().safe
    }

    /// Whether `null` is a value of `t`.
    pub fn nullable(&mut self, t: TypeId) -> bool {
        self.nullable_within(t, MAX_NULLABLE)
    }

    fn nullable_within(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.u.dealias(t);
        let widened = |s: &mut Self| {
            let w = s.u.widen(t);
            !s.explicit_nulls() && w != t && s.nullable_within(w, depth)
        };
        match *self.u.types().get(t) {
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

    /// Whether class `c` has `null` among its values: under safe nulls `Null`, `Any` or
    /// `Matchable`; else not `Nothing`, a value class or an object.
    fn nullable_class(&mut self, c: SymbolId) -> bool {
        let builtins = self.u.builtins();
        if self.safe_nulls() {
            return c == builtins.null || c == builtins.any || c == builtins.matchable;
        }
        c != builtins.nothing
            && !self.u.symbol(c).flags.has(OBJECT)
            && !builtins.any_val.is_some_and(|v| self.u.derives_from(c, v))
    }
}
