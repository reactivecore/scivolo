mod bounded;
mod constrain;
mod either;
mod pattern_type;
mod typed_pattern;
mod upcast;
mod upcasted;

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::TypeId;

/// Narrows the GADT bounds by one pattern against the selector's type: in
/// `def f[T](b: Box[T]): T = b match { case _: IntBox => 1 }` the case knows `T = Int`, and the
/// types a pattern such as `case b: Box[u]` binds get the bounds the selector gives them
/// (dotty's `constrainPatternType`). Its methods take the typer: comparing types completes the
/// aliases and classes they refer to.
pub struct Narrowing {
    ctx: Ctx,
}

impl Typer<'_> {
    /// Types pattern type tree `tpt` against `pt`, bounding the types it binds by what the
    /// pattern conforming to the selector requires of them, those then known.
    pub fn bounded_pattern_type(&mut self, tpt: TNodeId, pattern: bool, pt: Pt, ctx: Ctx) -> TypeId {
        Narrowing { ctx }.bounded_pattern_type(self, tpt, pattern, pt)
    }

    /// Narrows the GADT bounds by type pattern `pat` against `sel` where it does not conform to
    /// `sel` otherwise, as `case _: IntBox` against `Box[T]` (dotty's `constrainPatternType` in
    /// `typedTyped`).
    pub fn narrow_by_type_pattern(&mut self, pat: TypeId, sel: TypeId, ctx: Ctx) {
        Narrowing { ctx }.by_type_pattern(self, pat, sel);
    }

    /// Tells whether pattern type `pat` conforms to `sel`, constraining type variables; a union
    /// selector or pattern type by each operand.
    pub fn constrain_pattern(&mut self, pat: TypeId, sel: TypeId) -> Result<bool, &'static str> {
        constrain::constrain_pattern(self, pat, sel)
    }

    /// Constrains type variables and the GADT constraint by what pattern type `pat` matching a
    /// selector of type `sel` implies (dotty's `constrainPatternType`); false if they cannot
    /// share an instance.
    pub fn constrain_pattern_type(&mut self, pat: TypeId, sel: TypeId) -> bool {
        pattern_type::constrain_pattern_type(self, pat, sel)
    }
}
