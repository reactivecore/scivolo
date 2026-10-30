// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The right-hand sides of opaque types defined outside TASTy, seen only through the `this` of
//! their owner until the lowering elides them (dotty's `opaqueAlias`, `ElimOpaque`).

use super::{Kind, SymbolId, Universe};
use crate::tags::OPAQUE;
use crate::types::{Type, TypeId};

impl Universe {
    /// Makes `owner` the symbol whose definition comparisons are made in; the previous one.
    pub fn set_comparison_owner(&mut self, owner: Option<SymbolId>) -> Option<SymbolId> {
        std::mem::replace(&mut self.comparison_owner, owner)
    }

    /// How many comparisons succeeded through a lifted reference (dotty's `opaquesUsed`).
    pub fn opaque_lifts(&self) -> u64 {
        self.opaque_lifts
    }

    /// Counts a comparison that succeeded through a lifted reference.
    pub fn count_opaque_lift(&mut self) {
        self.opaque_lifts += 1;
    }

    /// The object class with opaque types that encloses the comparisons and whose object
    /// `module` is (dotty's `findEnclosingThis`).
    pub fn enclosing_opaque_module(&mut self, module: SymbolId) -> Option<SymbolId> {
        if !self.is_module(module) || self.comparison_owner.is_none() {
            return None;
        }
        let class = self.module_class(module)?;
        let mut from = self.comparison_owner;
        while let Some(s) = from.filter(|&s| self.symbol(s).kind != Kind::Package) {
            if s == class {
                return self.has_opaques(s).then_some(s);
            }
            from = self.symbol(s).owner;
        }
        None
    }

    /// Records `alias` as the right-hand side of opaque type `sym`, defined outside TASTy.
    pub fn set_opaque_alias(&mut self, sym: SymbolId, alias: TypeId) {
        self.opaque_aliases.insert(sym, alias);
    }

    /// The right-hand side of opaque type `s`, defined outside TASTy, whatever the prefix.
    pub fn opaque_alias(&self, s: SymbolId) -> Option<TypeId> {
        self.opaque_aliases.get(&s).copied()
    }

    /// The right-hand side of opaque type `s`, defined outside TASTy, if a reference through
    /// `prefix` sees it: through the `this` of its owner, through any prefix once elided.
    pub fn visible_opaque_alias(&mut self, s: SymbolId, prefix: Option<TypeId>) -> Option<TypeId> {
        if !self.symbol(s).flags.has(OPAQUE) {
            return None;
        }
        let alias = *self.opaque_aliases.get(&s)?;
        if self.opaques_elided {
            return Some(alias);
        }
        let Type::This(c) = *self.types().get(prefix?) else { return None };
        let owner = self.symbol(s).owner;
        let inside = self.comparison_owner.is_none_or(|o| self.encloses(owner, o));
        (self.class_symbol(c) == owner && inside).then_some(alias)
    }

    /// Whether the owner of `s` declares opaque types, or is an object whose owner does so,
    /// transitively (dotty's `seesOpaques`).
    pub fn sees_opaques(&self, s: SymbolId) -> bool {
        let mut owner = self.symbol(s).owner;
        while let Some(o) = owner {
            if self.symbol(o).kind == Kind::Class && self.symbol(o).flags.has(OPAQUE) {
                return true;
            }
            if !self.is_module_class(o) {
                return false;
            }
            owner = self.symbol(o).owner;
        }
        false
    }

    /// Whether a class enclosing `s` declares opaque types defined outside TASTy.
    pub fn within_opaques(&self, s: SymbolId) -> bool {
        let mut owner = self.symbol(s).owner;
        while let Some(o) = owner {
            if self.has_opaques(o) {
                return true;
            }
            owner = self.symbol(o).owner;
        }
        false
    }

    /// Whether class `c` declares opaque types defined outside TASTy (dotty's `containsOpaques`).
    pub fn has_opaques(&self, c: SymbolId) -> bool {
        self.symbol(c).flags.has(OPAQUE)
            && self.symbol(c).decls.iter().any(|&d| {
                let s = self.symbol(d);
                s.kind == Kind::Type && s.tree.is_none() && s.flags.has(OPAQUE)
            })
    }

    /// Whether `inner` is `outer` or owned by it, transitively.
    fn encloses(&self, outer: Option<SymbolId>, inner: SymbolId) -> bool {
        let mut s = Some(inner);
        while let Some(x) = s {
            if Some(x) == outer {
                return true;
            }
            s = self.symbol(x).owner;
        }
        false
    }

    /// What a reference to opaque type `s`, defined outside TASTy, through `prefix` stands for
    /// as an instance of a class: its right-hand side if visible, else its upper bound.
    pub fn opaque_underlying(&mut self, s: SymbolId, prefix: Option<TypeId>) -> Option<TypeId> {
        if let Some(alias) = self.visible_opaque_alias(s, prefix) {
            return Some(alias);
        }
        self.opaque_aliases.get(&s)?;
        let info = self.info(s);
        let Type::Bounds(_, hi) = *self.types().get(info) else { return None };
        Some(match (prefix, self.symbol(s).owner) {
            (Some(p), Some(owner)) => self.as_seen_from(hi, p, owner),
            _ => hi,
        })
    }
}
