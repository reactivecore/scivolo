//! The erasures remembered until the universe changes (ADR 0122).

use super::{SymbolId, Universe};
use crate::Erased;
use rustc_hash::FxHashMap;

/// Erased methods by method and whether value classes are semi-erased, erasures of references
/// to classes, and binary names of classes.
#[derive(Default)]
pub struct ErasureMemo {
    methods: FxHashMap<(SymbolId, bool), (Vec<Erased>, Erased)>,
    /// The erasures of references to classes that are not value classes.
    classes: FxHashMap<SymbolId, Erased>,
    binary_names: FxHashMap<SymbolId, String>,
}

impl ErasureMemo {
    /// Forgets every remembered erasure and binary name.
    pub fn clear(&mut self) {
        self.methods.clear();
        self.classes.clear();
        self.binary_names.clear();
    }
}

impl Universe {
    /// The erased parameters and result of method `def` remembered since the last change, with
    /// value classes semi-erased if `semi`.
    pub fn erased_memo(&self, def: SymbolId, semi: bool) -> Option<&(Vec<Erased>, Erased)> {
        self.erasures.methods.get(&(def, semi))
    }

    /// Remembers `erased` as `erased_memo`'s answer if the generation is still `generation`.
    pub fn remember_erased(
        &mut self,
        key: (SymbolId, bool),
        generation: u64,
        erased: &(Vec<Erased>, Erased),
    ) {
        if self.generation == generation {
            self.erasures.methods.insert(key, erased.clone());
        }
    }

    /// Determines `binary_name` of class `c`, remembered since the last change.
    pub fn erased_class_name(&mut self, c: SymbolId) -> String {
        if let Some(name) = self.erasures.binary_names.get(&c) {
            debug_assert_eq!(*name, self.binary_name(c), "remembered binary name");
            return name.clone();
        }
        let name = self.binary_name(c);
        self.erasures.binary_names.insert(c, name.clone());
        name
    }

    /// The erasure of a reference to class `c`, not a value class, remembered since the last
    /// change.
    pub fn erased_class_memo(&self, c: SymbolId) -> Option<Erased> {
        self.erasures.classes.get(&c).cloned()
    }

    /// Remembers `erased` as `erased_class_memo`'s answer for class `c` if the generation is
    /// still `generation`.
    pub fn remember_erased_class(&mut self, c: SymbolId, generation: u64, erased: &Erased) {
        if self.generation == generation {
            self.erasures.classes.insert(c, erased.clone());
        }
    }

    /// Drops every remembered erasure and starts a new generation.
    pub fn drop_erasures(&mut self) {
        self.erasures.clear();
        self.generation += 1;
    }
}
