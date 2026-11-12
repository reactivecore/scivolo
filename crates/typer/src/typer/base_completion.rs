//! Completion of the unit's classes before a base class test or an erasure.

use super::Typer;
use crate::symbols::Completion;
use tasty::{Kind, SymbolId, Type, TypeId};

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

    /// Completes the classes of the parameter and result types of method `def` and the underlying
    /// values of those that are value classes, which its erasure reads (dotty's
    /// `isDerivedValueClass` forces the class's info).
    pub fn complete_erased_classes(&mut self, def: SymbolId) {
        let mut t = self.u.info(def);
        let mut types = Vec::new();
        loop {
            match *self.u.ty(t) {
                Type::Poly { result, .. } => t = result,
                Type::Method { params, result, .. } => {
                    types.extend(self.u.params_of(params).iter().map(|&(_, p)| p));
                    t = result;
                }
                _ => break types.push(t),
            }
        }
        for t in types {
            let widened = self.u.widen(t);
            let Some(c) = self.u.class_symbol(widened) else { continue };
            self.complete(c);
            if let Some(value) = self.u.underlying(c) {
                self.complete(value);
            }
        }
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
