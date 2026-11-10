//! Inline arguments cast by GADT bounds outside the subset.

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Inlining<'_> {
    /// Refuses argument `arg` if it is cast by GADT bounds and is a singleton, whose expansion
    /// dotty types by the singleton, or the call's type refers to it by a skolem (`skolem`).
    pub fn check_gadt_arg(&mut self, arg: TNodeId, skolem: bool) -> Result<(), &'static str> {
        if !self.u.has_gadt_bounds() || !matches!(self.tpd.tree(arg), T::TypeApply { .. }) {
            return Ok(());
        }
        let tpe = self.tpd.tpe(arg);
        let singleton = matches!(*self.u.ty(tpe), Type::And(a, _)
            if self.u.sym_of(a).is_some_and(|s| self.u.symbol(s).kind.is_term()));
        match singleton || skolem {
            true => Err("inline argument cast by GADT bounds of a singleton or referred to by a skolem"),
            false => Ok(()),
        }
    }
}
