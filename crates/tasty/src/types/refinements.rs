// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The type members refinements give a path's type (dotty's `goRefined`, `lookupRefined`), and
//! types without their refinements.

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
                Type::And(a, b) => {
                    let (a, b) = (self.refinement_named(a, name), self.refinement_named(b, name));
                    let alias =
                        |i: &Option<TypeId>| i.is_some_and(|i| matches!(self.types().get(i), Type::Alias(_)));
                    return if alias(&b) || a.is_none() { b } else { a };
                }
                _ => match self.dealias(t) {
                    d if d != t => t = d,
                    _ => return None,
                },
            }
        }
        None
    }

    /// `t` without its refinements, and whether one of them refines a term member.
    pub fn without_refinements(&mut self, mut t: TypeId) -> (TypeId, bool) {
        let mut term = false;
        while let Type::Refined(p, _, info) = *self.types().get(t) {
            term |= !matches!(self.types().get(info), Type::Alias(_) | Type::Bounds(..));
            t = p;
        }
        (t, term)
    }

    /// `t` with its references to type members that refinements of `refined` alias replaced by
    /// their aliases.
    pub fn with_refined_aliases(&mut self, t: TypeId, refined: TypeId) -> TypeId {
        if !matches!(self.types().get(refined), Type::Refined(..)) {
            return t;
        }
        self.map_type(t, &mut |u, part| {
            let Type::Sym(Some(_), s) = *u.types().get(part) else { return None };
            let info = u.refinement_info(refined, s)?;
            match *u.types().get(info) {
                Type::Alias(a) => Some(a),
                _ => None,
            }
        })
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
