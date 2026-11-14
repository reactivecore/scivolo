//! Inline arguments and expansions cast by GADT bounds outside the subset.

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use tasty::Type;

impl Inlining<'_> {
    /// Refuses argument `arg` if it is cast by GADT bounds and the call's type refers to it by a
    /// skolem (`skolem`).
    pub fn check_gadt_arg(&mut self, arg: TNodeId, skolem: bool) -> Result<(), &'static str> {
        match skolem && self.u.has_gadt_bounds() && matches!(self.tpd.tree(arg), T::TypeApply { .. }) {
            true => Err("inline argument cast by GADT bounds referred to by a skolem"),
            false => Ok(()),
        }
    }

    /// Refuses `t` if it casts a singleton by GADT bounds, as `x.asInstanceOf[x.type & Int & T]`
    /// in the body of `inline def f[T](x: T): T` with `T = Int`, whose type in an expansion dotty
    /// simplifies.
    pub fn check_gadt_cast(&mut self, t: TNodeId) -> Result<(), &'static str> {
        let T::TypeApply { fun, .. } = *self.tpd.tree(t) else { return Ok(()) };
        let is_cast = self.tpd.sym(fun).is_some_and(|m| self.u.symbol(m).name == nme::AS_INSTANCE_OF_PM);
        let mut target = self.tpd.tpe(t);
        while let Some((a, _)) = self.u.annotated(target) {
            target = a;
        }
        let Type::And(mut first, _) = *self.u.ty(target) else { return Ok(()) };
        while let Type::And(a, _) | Type::Annotated(a, _) = *self.u.ty(first) {
            first = a;
        }
        let singleton = self.u.sym_of(first).is_some_and(|s| self.u.symbol(s).kind.is_term())
            || matches!(
                self.u.ty(first),
                Type::Constant(..) | Type::Str(_) | Type::This(_) | Type::TermRef(..)
            );
        match is_cast && singleton {
            true => Err("GADT cast of a singleton in an inline expansion"),
            false => Ok(()),
        }
    }
}
