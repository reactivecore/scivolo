//! The type members refinements give a path's type (dotty's `goRefined`, `lookupRefined`).

use super::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

/// How many steps finding a refinement takes at most.
const MAX_STEPS: u32 = 64;

impl Universe {
    /// The info a refinement of `prefix`'s widened type gives type member `s`.
    pub fn refinement_info(&mut self, prefix: TypeId, s: SymbolId) -> Option<TypeId> {
        if self.symbol(s).kind != Kind::Type {
            return None;
        }
        let name = self.symbol(s).name.clone();
        self.refinement_named(prefix, &name)
    }

    /// The info a refinement of `prefix`'s widened type gives type member `name`, a member
    /// that need not have a symbol: its bounds or alias.
    pub fn refinement_named(&mut self, prefix: TypeId, name: &str) -> Option<TypeId> {
        let mut t = self.widen(prefix);
        for _ in 0..MAX_STEPS {
            match self.types().get(t).clone() {
                Type::Refined(_, n, info)
                    if n == name && matches!(self.types().get(info), Type::Alias(_) | Type::Bounds(..)) =>
                {
                    return Some(info);
                }
                Type::Refined(p, ..) | Type::Annotated(p, _) => t = p,
                _ => match self.dealias(t) {
                    d if d != t => t = d,
                    _ => return None,
                },
            }
        }
        None
    }

    /// The info a refinement of `prefix`'s widened type gives term member `name`.
    pub fn term_refinement_named(&mut self, prefix: TypeId, name: &str) -> Option<TypeId> {
        let mut t = self.widen(prefix);
        for _ in 0..MAX_STEPS {
            match self.types().get(t).clone() {
                Type::Refined(_, n, info)
                    if n == name && !matches!(self.types().get(info), Type::Alias(_) | Type::Bounds(..)) =>
                {
                    return Some(info);
                }
                Type::Refined(p, ..) | Type::Annotated(p, _) | Type::Rec(_, p) => t = p,
                _ => match self.dealias(t) {
                    d if d != t => t = d,
                    _ => return None,
                },
            }
        }
        None
    }
}
