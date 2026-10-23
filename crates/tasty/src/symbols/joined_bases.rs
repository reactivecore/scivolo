// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Base types that several parents of a class give a base class, intersected (dotty's
//! `baseTypeOf` folding the parents with `&`, `glbArgs`).

use super::{SymbolId, Universe};
use crate::tags::{CONTRAVARIANT, COVARIANT};
use crate::types::{Type, TypeId};

impl Universe {
    /// Whether two parents of class `c` give its base class `base` different types.
    pub fn parents_disagree(&mut self, c: SymbolId, base: SymbolId) -> bool {
        if let Some(&d) = self.disagreeing_bases.get(&(c, base)) {
            return d;
        }
        self.disagreeing_bases.insert((c, base), false);
        let mut first = None;
        let mut disagree = false;
        if !self.type_params(base).is_empty() {
            for parent in self.parents(c) {
                if !self.class_symbol(parent).is_some_and(|p| self.derives_from(p, base)) {
                    continue;
                }
                let Some(found) = self.base_type(parent, base) else { continue };
                disagree |= *first.get_or_insert(found) != found;
            }
        }
        self.disagreeing_bases.insert((c, base), disagree);
        disagree
    }

    /// Base types `a` and `b` of class `base` intersected, argument by argument as their
    /// parameters' variances allow; `None` if an invariant argument differs.
    pub fn joined_base_type(&mut self, base: SymbolId, a: TypeId, b: TypeId) -> Option<TypeId> {
        let (Type::Applied(tycon, xs), Type::Applied(_, ys)) =
            (self.types().get(a).clone(), self.types().get(b).clone())
        else {
            return (a == b).then_some(a);
        };
        let params = self.type_params(base);
        let mut args = Vec::with_capacity(xs.len());
        for ((x, y), p) in xs.into_iter().zip(ys).zip(params) {
            let flags = self.symbol(p).flags;
            let (co, contra) = (flags.has(COVARIANT), flags.has(CONTRAVARIANT));
            let below = x == y || self.is_subtype(x, y);
            let above = x == y || self.is_subtype(y, x);
            args.push(match (below, above) {
                (true, true) => x,
                _ if !co && !contra => return None,
                (true, false) | (false, true) if co == below => x,
                (true, false) | (false, true) => y,
                _ if co => self.intern_type(Type::And(x, y)),
                _ => self.intern_type(Type::Or(x, y, true)),
            });
        }
        Some(self.intern_type(Type::Applied(tycon, args)))
    }
}
