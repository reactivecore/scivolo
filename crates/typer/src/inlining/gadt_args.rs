//! Inline arguments cast by GADT bounds outside the subset.

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Inlining<'_> {
    /// Refuses argument `arg` if it is a singleton cast by GADT bounds, whose expansion dotty
    /// types by the singleton.
    pub fn check_gadt_arg(&mut self, arg: TNodeId) -> Result<(), &'static str> {
        let tpe = self.tpd.tpe(arg);
        if self.u.has_gadt_bounds()
            && matches!(self.tpd.tree(arg), T::TypeApply { .. })
            && let Type::And(a, _) = *self.u.types().get(tpe)
            && matches!(*self.u.types().get(a), Type::Sym(_, s) if self.u.symbol(s).kind.is_term())
        {
            return Err("inline argument of a singleton cast by GADT bounds");
        }
        Ok(())
    }
}
