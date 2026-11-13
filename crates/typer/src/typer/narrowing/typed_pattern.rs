// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The GADT bounds a binder-less typed pattern gives the selector's type parameters (dotty's
//! `constrainPatternType` in `typedTyped`).

use super::super::Typer;
use super::Narrowing;
use super::pattern_type::constrain_pattern_type;
use tasty::TypeId;

impl Narrowing {
    /// Narrows the GADT bounds by type pattern `pat` against `sel` where it does not conform to
    /// `sel` otherwise, in a case (dotty's `constrainPatternType` in `typedTyped`).
    pub fn by_type_pattern(&self, t: &mut Typer, pat: TypeId, sel: TypeId) {
        if t.gadt.cases > 0 {
            t.gadt.register(t.u, self.ctx);
            constrain_pattern_type(t, pat, sel);
        }
    }
}
