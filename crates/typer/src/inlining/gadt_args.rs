//! Inline arguments cast by GADT bounds outside the subset.

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};

impl Inlining<'_> {
    /// Refuses argument `arg` if it is cast by GADT bounds and the call's type refers to it by a
    /// skolem (`skolem`).
    pub fn check_gadt_arg(&mut self, arg: TNodeId, skolem: bool) -> Result<(), &'static str> {
        match skolem && self.u.has_gadt_bounds() && matches!(self.tpd.tree(arg), T::TypeApply { .. }) {
            true => Err("inline argument cast by GADT bounds referred to by a skolem"),
            false => Ok(()),
        }
    }
}
