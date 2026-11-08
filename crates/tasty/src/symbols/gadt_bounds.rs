//! The bounds a pattern gives type parameters while its case is typed (dotty's
//! `GadtConstraint`), beside their declared infos.

use super::{SymbolId, Universe};
use crate::types::TypeId;

impl Universe {
    /// Sets or removes the GADT bounds of `sym`; the old ones.
    pub fn set_gadt_bounds(
        &mut self,
        sym: SymbolId,
        bounds: Option<(TypeId, TypeId)>,
    ) -> Option<(TypeId, TypeId)> {
        match bounds {
            Some(b) => self.gadt_bounds.insert(sym, b),
            None => self.gadt_bounds.remove(&sym),
        }
    }

    /// Whether any symbol has GADT bounds.
    pub fn has_gadt_bounds(&self) -> bool {
        !self.gadt_bounds.is_empty()
    }

    /// The GADT bounds of `sym`, if it has some.
    pub fn gadt_bounds(&self, sym: SymbolId) -> Option<(TypeId, TypeId)> {
        self.gadt_bounds.get(&sym).copied()
    }
}
