//! The atoms and `Nothing`-ness one walk over a type has computed, by type and depth, each
//! with whether it was provisional.

use super::atoms::Atoms;
use crate::types::TypeId;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct Walk {
    atoms: FxHashMap<(TypeId, u32), (Atoms, bool)>,
    nothing: FxHashMap<(TypeId, u32), (Option<bool>, bool)>,
}

impl Walk {
    /// Forgets everything, for a walk under a constraint that may differ.
    pub fn clear(&mut self) {
        if !self.atoms.is_empty() || !self.nothing.is_empty() {
            self.atoms.clear();
            self.nothing.clear();
        }
    }

    pub fn atoms(&self, t: TypeId, depth: u32) -> Option<(Atoms, bool)> {
        self.atoms.get(&(t, depth)).cloned()
    }

    pub fn set_atoms(&mut self, t: TypeId, depth: u32, atoms: Atoms, provisional: bool) {
        self.atoms.insert((t, depth), (atoms, provisional));
    }

    pub fn nothing(&self, t: TypeId, depth: u32) -> Option<(Option<bool>, bool)> {
        self.nothing.get(&(t, depth)).copied()
    }

    pub fn set_nothing(&mut self, t: TypeId, depth: u32, nothing: Option<bool>, provisional: bool) {
        self.nothing.insert((t, depth), (nothing, provisional));
    }
}
