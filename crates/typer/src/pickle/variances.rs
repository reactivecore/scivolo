// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! The variances after the bounds of a type lambda (dotty's `pickleVariances`).

use super::{Refusal, TreePickler};
use tasty::TypeId;
use tasty::tags::{CONTRAVARIANT, COVARIANT, STABLE};

impl TreePickler<'_> {
    /// The variances type lambda `hi`, the upper bound of bounds if `bounds` else an alias,
    /// declares; none for another type or a lambda the typer made whose variances are
    /// structural.
    pub fn variances(&self, hi: TypeId, bounds: bool) -> Result<Vec<i8>, Refusal> {
        let Some(binder) = self.u.lambda(hi).map(|l| l.binder) else { return Ok(Vec::new()) };
        match self.u.declared_variances(binder) {
            Some(v) => Ok(v),
            None if bounds || self.u.is_file_binder(binder) => {
                Err("bounds of a type lambda, whose variances are not known")
            }
            None => Ok(Vec::new()),
        }
    }

    /// Writes `variances`, +1, -1 or 0 each.
    pub fn write_variances(&mut self, variances: &[i8]) {
        for &v in variances {
            self.byte(match v {
                1 => COVARIANT,
                -1 => CONTRAVARIANT,
                _ => STABLE,
            });
        }
    }
}
