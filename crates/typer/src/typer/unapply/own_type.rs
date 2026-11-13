// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The type of an extractor pattern (dotty's `ownType` in `typedUnApply`).

use super::super::Typer;
use super::UnapplyPattern;
use crate::tpd::TNodeId;
use tasty::{TypeId, TypeVars};

impl UnapplyPattern {
    /// Infers the type of the pattern whose extractor `unapp` takes `arg_type`: the scrutinee's
    /// type if it conforms, else `arg_type` constrained by it in GADT mode and maximized with the
    /// binders added to `unapp`; type variables from id `vars` on instantiated. Refuses the
    /// latter outside a case.
    pub fn pattern_type(
        &self,
        t: &mut Typer<'_>,
        arg_type: TypeId,
        vars: usize,
        unapp: TNodeId,
    ) -> Result<TypeId, &'static str> {
        let sel = self.sel;
        let mark = t.constraint.mark();
        if t.conforms(sel, arg_type) {
            t.fully_defined_as(arg_type, true)?;
            return Ok(if t.is_bottom_type(sel) { t.zonk(arg_type) } else { t.without_unchecked(sel) });
        }
        t.constraint.rollback(mark);

        if t.gadt.cases == 0 {
            return Err("extractor pattern of another type than the selector's outside a case");
        }
        let level = t.gadt.register(t.u, self.ctx);
        t.constrain_pattern_type(arg_type, sel);
        let bound = t.maximize_type(arg_type, vars, unapp, level, self.ctx)?;
        t.gadt.bind(&bound);
        t.add_binders(unapp, &bound);
        Ok(t.zonk(arg_type))
    }
}
