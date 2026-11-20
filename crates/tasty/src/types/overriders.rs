// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Abstract type members as a subclass of their owner overrides them.

use super::{Type, TypeId};
use crate::tags::PRIVATE;
use crate::{ANY, Kind, NOTHING, SymbolId, Universe};

impl Universe {
    /// The alias or class by which the class of term `prefix`'s type as seen from its own prefix,
    /// or of `prefix` a `this`, overrides abstract type member `s`, seen from `prefix`: the first
    /// of `overriders` that is one.
    pub fn overriding_alias(&mut self, prefix: TypeId, s: SymbolId) -> Option<TypeId> {
        for (m, owner) in self.overriders(prefix, s) {
            if self.symbol(m).kind == Kind::Class {
                return Some(self.intern_type(Type::Sym(Some(prefix), m)));
            }
            if let Some(alias) = self.alias_of(m) {
                return Some(self.as_seen_from(alias, prefix, owner));
            }
        }
        None
    }

    /// Intersects the bounds of abstract type member `s` and of the abstract types overriding
    /// it in the class of `prefix`, seen from `prefix` (dotty's `Denotation.meet` of the
    /// members named alike): `Lambda` as the upper bound of `binder`'s type `BT` in
    /// `class ParamRef extends BoundType { type BT <: Lambda }`, `CharSequence & Comparable[String]`
    /// for `T` in `abstract class E extends B with C` of a `B` with `type T <: CharSequence` and
    /// a `C` with `type T <: Comparable[String]`. `None` without an overrider, or if one is an
    /// alias or a class.
    pub fn overriding_bounds(&mut self, prefix: TypeId, s: SymbolId) -> Option<TypeId> {
        let overriders = self.overriders(prefix, s);
        if overriders.is_empty() {
            return None;
        }
        let mut met = self.symbol(s).owner.and_then(|base| self.bounds_seen_from(s, base, prefix));
        for (m, owner) in overriders {
            if self.symbol(m).kind != Kind::Type {
                return None;
            }
            let (lo, hi) = self.bounds_seen_from(m, owner, prefix)?;
            met = Some(match met {
                Some((lo2, hi2)) => {
                    (self.met_side(lo2, lo, NOTHING, false), self.met_side(hi2, hi, ANY, true))
                }
                None => (lo, hi),
            });
        }
        let (lo, hi) = met?;
        Some(self.intern_type(Type::Bounds(lo, hi)))
    }

    /// Joins bounds `a` and `b` without comparing them: `a & b` if `and`, else `a | b`; one of
    /// them if they are equal or the other is `trivial`.
    fn met_side(&mut self, a: TypeId, b: TypeId, trivial: TypeId, and: bool) -> TypeId {
        match () {
            _ if a == b || b == trivial => a,
            _ if a == trivial => b,
            _ if and => self.intern_type(Type::And(a, b)),
            _ => self.intern_type(Type::Or(a, b, false)),
        }
    }

    /// The bounds of abstract type member `m` of class `owner`, seen from `prefix`.
    fn bounds_seen_from(&mut self, m: SymbolId, owner: SymbolId, prefix: TypeId) -> Option<(TypeId, TypeId)> {
        let info = self.info(m);
        if !matches!(self.ty(info), Type::Bounds(..)) {
            return None;
        }
        let seen = self.as_seen_from(info, prefix, owner);
        match *self.ty(seen) {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            _ => None,
        }
    }

    /// Finds the type members and classes named as abstract type member `s` that the class of
    /// `prefix`'s type and its base classes declare, other than `s` and those of `s`'s owner's
    /// base classes, with their owners, in linearization order.
    pub fn overriders(&mut self, prefix: TypeId, s: SymbolId) -> Vec<(SymbolId, SymbolId)> {
        let Some((cls, base)) = self.overriding_class(prefix, s) else { return Vec::new() };
        let name = self.interned_name(s);
        let mut found = Vec::new();
        for c in self.linearization(cls) {
            if self.derives_from(base, c) {
                continue;
            }
            for m in self.decl(c, name).unwrap_or_default() {
                let private = c != cls && self.symbol(m).flags.has(PRIVATE);
                if !private && matches!(self.symbol(m).kind, Kind::Type | Kind::Class) {
                    found.push((m, c));
                }
            }
        }
        found
    }

    /// The class of term `prefix`'s type, of `prefix` a `this`, or `prefix` a class type (the
    /// prefix of a projection `C#T`), if it is a proper subclass of type member `s`'s owner, and
    /// that owner.
    fn overriding_class(&mut self, prefix: TypeId, s: SymbolId) -> Option<(SymbolId, SymbolId)> {
        if self.symbol(s).kind != Kind::Type {
            return None;
        }
        let declared = match *self.ty(prefix) {
            Type::Sym(p, term) if self.symbol(term).kind.is_term() => self.term_type(p, term),
            Type::TermRef(p, _) => {
                let term = self.resolve_ref(prefix)?;
                self.term_type(Some(p), term)
            }
            Type::This(c) => c,
            _ => prefix,
        };
        let base = self.symbol(s).owner?;
        let declared = self.widen(declared);
        let cls = self.class_symbol(declared).filter(|&c| c != base && self.derives_from(c, base))?;
        Some((cls, base))
    }
}
