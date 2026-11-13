// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/core/OrderingConstraint.scala
//! The GADT constraint (ADR 0119): what the cases being typed know of the type parameters and
//! pattern-bound types they narrow, as bounds and an ordering between those symbols.

mod order;
mod update;

pub use order::Unification;
pub use update::Registered;

use crate::subtype::Mark;
use crate::{Kind, SymbolId, Type, TypeId, Universe};

/// Holds the symbols a case may narrow with their bounds and ordering (dotty's
/// `GadtConstraint` over an `OrderingConstraint`, keyed by symbol). Every change is logged so
/// that a failed comparison or a finished case rolls back to a `Mark`.
#[derive(Default, Clone)]
pub struct GadtConstraint {
    /// The registered symbols with their entries, in registration order.
    entries: Vec<(SymbolId, Entry)>,
    /// Whether a bound was narrowed since the constraint was empty (dotty's `wasConstrained`).
    narrowed: bool,
    log: Vec<Undo>,
    /// Whether the comparisons running narrow the constraint (dotty's
    /// `Mode.GadtConstraintInference`).
    pub inferring: bool,
}

/// What the constraint knows of one symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The bounds without the registered symbols they are ordered with.
    pub lo: TypeId,
    pub hi: TypeId,
    /// The type the symbol was made equal to by `replace`.
    pub instance: Option<TypeId>,
    /// The registered symbols below this one, transitively, most recently ordered first.
    pub lower: Vec<SymbolId>,
    /// The registered symbols above this one, transitively, most recently ordered first.
    pub upper: Vec<SymbolId>,
    /// The scope nesting level the symbol was registered at; `unify` keeps the shallower.
    pub level: u32,
    /// Whether a comparison changed the entry since the symbol was registered.
    pub narrowed: bool,
}

/// One change to undo.
#[derive(Clone)]
enum Undo {
    Changed(usize, Entry),
    Added,
    Removed(usize, SymbolId, Entry),
    Narrowed,
}

/// A state of the constraint that `redo` restores after a rollback.
#[derive(Clone, PartialEq, Eq)]
pub struct GadtChanges {
    entries: Vec<(SymbolId, Entry)>,
    narrowed: bool,
}

impl GadtChanges {
    /// Gets the entry of `s` in the captured state.
    pub fn entry(&self, s: SymbolId) -> Option<&Entry> {
        self.entries.iter().find(|&&(e, _)| e == s).map(|(_, e)| e)
    }
}

impl GadtConstraint {
    /// Tells whether no symbol is registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Tells whether `s` is registered, instantiated or not (dotty's `contains`).
    pub fn contains(&self, s: SymbolId) -> bool {
        self.index(s).is_some()
    }

    /// Lists the registered symbols in registration order (dotty's `symbols`).
    pub fn symbols(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.entries.iter().map(|&(s, _)| s)
    }

    /// Tells whether a bound was narrowed (dotty's `isNarrowing`).
    pub fn is_narrowing(&self) -> bool {
        self.narrowed
    }

    /// Records that a bound was narrowed (dotty's `withWasConstrained`).
    pub fn set_narrowing(&mut self) {
        if !self.narrowed {
            self.narrowed = true;
            self.log.push(Undo::Narrowed);
        }
    }

    /// Gets the entry of `s`.
    pub fn entry(&self, s: SymbolId) -> Option<&Entry> {
        self.index(s).map(|i| &self.entries[i].1)
    }

    /// Gives the bounds of `s` without the symbols it is ordered with, both its instance if it
    /// has one (dotty's `bounds`, `nonParamBounds`).
    pub fn bounds(&self, s: SymbolId) -> Option<(TypeId, TypeId)> {
        let e = self.entry(s)?;
        Some(e.instance.map_or((e.lo, e.hi), |i| (i, i)))
    }

    pub fn mark(&self) -> Mark {
        Mark(self.log.len())
    }

    /// Undoes the changes made after `m`.
    pub fn rollback(&mut self, m: Mark) {
        while self.log.len() > m.0 {
            match self.log.pop() {
                Some(Undo::Changed(i, e)) => self.entries[i].1 = e,
                Some(Undo::Added) => drop(self.entries.pop()),
                Some(Undo::Removed(i, s, e)) => self.entries.insert(i, (s, e)),
                Some(Undo::Narrowed) => self.narrowed = false,
                None => {}
            }
        }
    }

    /// Captures the current state for `redo` after a rollback to `m`, or `None` if nothing
    /// changed since `m`.
    pub fn changes_since(&self, m: Mark) -> Option<GadtChanges> {
        (self.log.len() > m.0).then(|| GadtChanges { entries: self.entries.clone(), narrowed: self.narrowed })
    }

    /// Restores the state `changes` captured, logging each change.
    pub fn redo(&mut self, changes: GadtChanges) {
        while let Some(i) =
            self.entries.iter().position(|(s, _)| !changes.entries.iter().any(|(c, _)| c == s))
        {
            self.remove(self.entries[i].0);
        }
        for (s, e) in changes.entries {
            self.set_entry(s, e);
        }
        if changes.narrowed {
            self.set_narrowing();
        }
    }

    /// Sets the entry of `s`, registering it if it is new.
    fn set_entry(&mut self, s: SymbolId, e: Entry) {
        match self.index(s) {
            Some(i) if self.entries[i].1 == e => {}
            Some(i) => {
                let old = std::mem::replace(&mut self.entries[i].1, e);
                self.log.push(Undo::Changed(i, old));
            }
            None => {
                self.entries.push((s, e));
                self.log.push(Undo::Added);
            }
        }
    }

    /// Removes `s` from the constraint.
    fn remove(&mut self, s: SymbolId) {
        let Some(i) = self.index(s) else { return };
        let (s, e) = self.entries.remove(i);
        self.log.push(Undo::Removed(i, s, e));
    }

    fn index(&self, s: SymbolId) -> Option<usize> {
        self.entries.iter().position(|&(e, _)| e == s)
    }

    /// Marks the entries changed since `m` as narrowed.
    pub fn mark_narrowed_since(&mut self, m: Mark) {
        let mut changed: Vec<usize> = self.log[m.0..]
            .iter()
            .filter_map(|u| match u {
                Undo::Changed(i, _) => Some(*i),
                _ => None,
            })
            .collect();
        changed.sort_unstable();
        changed.dedup();
        for i in changed {
            let s = self.entries[i].0;
            self.modify(s, |e| e.narrowed = true);
        }
    }

    /// Changes the entry of registered `s` by `f`.
    fn modify(&mut self, s: SymbolId, f: impl FnOnce(&mut Entry)) {
        let Some(mut e) = self.entry(s).cloned() else { return };
        f(&mut e);
        self.set_entry(s, e);
    }
}

impl Universe {
    /// Finds the registered symbol `t` refers to at its top level, plainly or seen from `this`
    /// of its owner (dotty's internalized `TypeParamRef`).
    pub fn gadt_param(&mut self, t: TypeId) -> Option<SymbolId> {
        if self.gadt().is_empty() {
            return None;
        }
        let (prefix, s) = self.sym_ref(t)?;
        if let Some(p) = prefix {
            let c = self.this_of(p)?;
            (self.class_symbol(c) == self.symbol(s).owner).then_some(())?;
        }
        self.gadt().contains(s).then_some(s)
    }

    /// Makes a reference to type `s`, a class's type parameter seen from `this`.
    pub fn gadt_ref(&mut self, s: SymbolId) -> TypeId {
        let of_class =
            self.symbol(s).owner.filter(|&o| self.symbol(s).kind == Kind::TypeParam && self.is_class(o));
        let prefix = of_class.map(|c| {
            let class = self.intern_type(Type::Sym(None, c));
            self.intern_type(Type::This(class))
        });
        self.intern_type(Type::Sym(prefix, s))
    }

    /// Whether a comparison narrowed a symbol.
    pub fn has_gadt_bounds(&self) -> bool {
        self.gadt().entries.iter().any(|(_, e)| e.narrowed)
    }

    /// Gives the GADT bounds of `sym` if a comparison narrowed it, with the symbols it is
    /// ordered with (dotty's `fullBounds`).
    pub fn gadt_bounds(&mut self, sym: SymbolId) -> Option<(TypeId, TypeId)> {
        let e = self.gadt().entry(sym).filter(|e| e.narrowed)?;
        if let Some(i) = e.instance {
            return Some((i, i));
        }
        let (mut lo, mut hi) = (e.lo, e.hi);
        for p in self.gadt().min_lower(sym) {
            let r = self.gadt_ref(p);
            lo = self.lub(lo, r);
        }
        for p in self.gadt().min_upper(sym) {
            let r = self.gadt_ref(p);
            hi = if hi == crate::ANY { r } else { self.glb(hi, r) };
        }
        Some((lo, hi))
    }

    /// Sets or removes the GADT bounds of `sym` as they are, unordered; the old ones.
    pub fn set_gadt_bounds(
        &mut self,
        sym: SymbolId,
        bounds: Option<(TypeId, TypeId)>,
    ) -> Option<(TypeId, TypeId)> {
        let old = self.gadt_bounds(sym);
        match bounds {
            Some((lo, hi)) => {
                let (lower, upper) = (Vec::new(), Vec::new());
                let e = Entry { lo, hi, instance: None, lower, upper, level: 0, narrowed: true };
                self.gadt_mut().set_entry(sym, e);
            }
            None => self.gadt_mut().remove(sym),
        }
        old
    }
}
