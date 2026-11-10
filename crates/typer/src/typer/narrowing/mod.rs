mod bounded;
mod constrain;
mod order;
mod tuples;
mod typed_pattern;
mod upcast;
mod vars;

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{SymbolId, TypeId};

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

    /// Finds the type parameters of `sel` that pattern type `pat` narrows, with the type
    /// variables standing for them while `pat` is compared with `sel`; the type variables from
    /// id `from` on are the pattern's own. `None` if the pattern does not conform or narrows
    /// nothing.
    pub fn gadt_vars(
        &mut self,
        (pat, from): (TypeId, usize),
        sel: TypeId,
        ctx: Ctx,
    ) -> Result<Option<Vec<(SymbolId, TypeId)>>, &'static str> {
        Narrowing { ctx }.vars(self, (pat, from), sel)
    }

    /// Narrows each type parameter of `vars` to the bounds its variable got while the case is
    /// typed; refused for a bound the comparison left undetermined.
    pub fn narrow_gadt(&mut self, vars: &[(SymbolId, TypeId)]) -> Result<(), &'static str> {
        vars::narrow(self, vars)
    }
}
