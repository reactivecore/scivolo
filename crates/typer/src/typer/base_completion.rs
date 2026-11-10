//! Completion of the unit's classes before a base class test.

use super::Typer;
use crate::symbols::Completion;
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    /// Whether `t` derives from class `class`, given by its full name, the unit's classes among
    /// its base classes completed first.
    pub fn derives_from_named(&mut self, t: TypeId, class: &str) -> bool {
        let base = self.resolve_full_name(class, true);
        let t = self.u.widen(t);
        match (self.u.class_symbol(t), base) {
            (Some(c), Some(b)) => {
                self.complete_bases(c);
                self.u.derives_from(c, b)
            }
            _ => false,
        }
    }

    /// Completes the unit's classes not yet completed, members of packages and classes, whose
    /// parents a library class may reach, unless a definition is being completed; whether there
    /// were any.
    pub fn complete_unit_classes(&mut self) -> bool {
        if self.syms.completions.values().any(|c| matches!(c, Completion::Completing(_))) {
            return false;
        }
        let mut pending: Vec<SymbolId> = (self.syms.completions.iter())
            .filter(|&(&s, c)| {
                matches!(c, Completion::Pending { .. }) && self.u.symbol(s).kind == Kind::Class
            })
            .filter(|&(&s, _)| self.u.symbol(s).owner.is_some_and(|o| self.u.symbol(o).kind != Kind::Def))
            .map(|(&s, _)| s)
            .collect();
        pending.sort_by_key(|s| s.0);
        pending.iter().for_each(|&s| self.complete(s));
        !pending.is_empty()
    }

    /// Completes class `c` and the unit's classes among its base classes.
    fn complete_bases(&mut self, c: SymbolId) {
        loop {
            let pending: Vec<SymbolId> = std::iter::once(c)
                .chain(self.u.linearization(c))
                .filter(|s| matches!(self.syms.completions.get(s), Some(Completion::Pending { .. })))
                .collect();
            if pending.is_empty() {
                return;
            }
            pending.into_iter().for_each(|s| self.complete(s));
        }
    }
}
