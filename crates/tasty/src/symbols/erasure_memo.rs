//! The erased methods remembered between changes of the universe (ADR 0122).

use super::{Kind, Symbol, SymbolId, Universe};
use crate::tags::{OBJECT as MODULE, PARAMsetter};
use crate::{Erased, NameId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Erased methods by method and whether value classes are semi-erased, the names their
/// erasures looked up, and binary names of classes.
#[derive(Default)]
pub struct ErasureMemo {
    methods: FxHashMap<(SymbolId, bool), (Vec<Erased>, Erased)>,
    names: FxHashSet<NameId>,
    /// How many erasures of methods are being computed, one inside another.
    erasing: u32,
    binary_names: FxHashMap<SymbolId, String>,
    /// The erasures of references to classes that are not value classes.
    classes: FxHashMap<SymbolId, Erased>,
}

impl ErasureMemo {
    /// Forgets every erased method and binary name.
    pub fn clear(&mut self) {
        self.methods.clear();
        self.names.clear();
        self.binary_names.clear();
        self.classes.clear();
    }

    /// Records that members named `name` were looked up, if while an erasure is computed.
    pub fn looked_up(&mut self, name: NameId) {
        if self.erasing > 0 {
            self.names.insert(name);
        }
    }

    /// Determines whether an erasure is being computed.
    pub fn is_erasing(&self) -> bool {
        self.erasing > 0
    }

    /// Determines whether declaration `s`, named `name`, defined, entered or unentered, can
    /// change a remembered erasure: unless it is a method or value (not an object, not a class
    /// parameter's accessor) without a run-time name whose name no remembered erasure looked up.
    pub fn changed_by(&self, s: &Symbol, name: NameId) -> bool {
        let term = match s.kind {
            Kind::Def => true,
            Kind::Val => !s.flags.has(MODULE) && !s.flags.has(PARAMsetter),
            _ => false,
        };
        !term || s.target.is_some() || self.names.contains(&name)
    }
}

impl Universe {
    /// The erased parameters and result of method `def` remembered since the last change, with
    /// value classes semi-erased if `semi`.
    pub fn erased_memo(&self, def: SymbolId, semi: bool) -> Option<&(Vec<Erased>, Erased)> {
        self.erased_methods.methods.get(&(def, semi))
    }

    /// Remembers `erased` as `erased_memo`'s answer if the generation is still `generation`.
    pub fn remember_erased(
        &mut self,
        key: (SymbolId, bool),
        generation: u64,
        erased: &(Vec<Erased>, Erased),
    ) {
        if self.generation == generation {
            self.erased_methods.methods.insert(key, erased.clone());
        }
    }

    /// Determines `binary_name` of class `c`, remembered since the last change of a symbol it
    /// depends on.
    pub fn erased_class_name(&mut self, c: SymbolId) -> String {
        if let Some(name) = self.erased_methods.binary_names.get(&c) {
            debug_assert_eq!(*name, self.binary_name(c), "remembered binary name");
            return name.clone();
        }
        let name = self.binary_name(c);
        self.erased_methods.binary_names.insert(c, name.clone());
        name
    }

    /// The erasure of a reference to class `c`, not a value class, remembered since the last
    /// change.
    pub fn erased_class_memo(&self, c: SymbolId) -> Option<Erased> {
        self.erased_methods.classes.get(&c).cloned()
    }

    /// Remembers `erased` as `erased_class_memo`'s answer for class `c`.
    pub fn remember_erased_class(&mut self, c: SymbolId, erased: &Erased) {
        self.erased_methods.classes.insert(c, erased.clone());
    }

    /// Runs `erase`, an erasure of a method that `remember_erased` may remember, recording the
    /// names it looks up.
    pub fn while_erasing<T>(&mut self, erase: impl FnOnce(&mut Self) -> T) -> T {
        self.erased_methods.erasing += 1;
        let erased = erase(self);
        self.erased_methods.erasing -= 1;
        erased
    }
}
