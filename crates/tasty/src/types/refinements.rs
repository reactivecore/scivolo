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
    /// that need not have a symbol: bounds, or an alias, which TASTy gives as the bare type.
    pub fn refinement_named(&mut self, prefix: TypeId, name: &str) -> Option<TypeId> {
        let mut t = self.widen(prefix);
        for _ in 0..MAX_STEPS {
            match self.types().get(t).clone() {
                Type::Refined(_, n, info) if n == name && !is_term_info(self.types().get(info)) => {
                    return match self.types().get(info) {
                        Type::Alias(_) | Type::Bounds(..) => Some(info),
                        _ => Some(self.intern_type(Type::Alias(info))),
                    };
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
}

/// Whether refinement info `t` is a method's or by-name value's.
fn is_term_info(t: &Type) -> bool {
    matches!(t, Type::Method { .. } | Type::Poly { .. } | Type::ByName(_))
}
