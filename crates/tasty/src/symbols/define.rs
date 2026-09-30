//! Symbols defined outside TASTy (a script's), what completes them (infos, parents, flags),
//! and binders for the types made with them.

use super::{FileId, Flags, Symbol, SymbolId, Universe};
use crate::types::{Binder, TypeId};

impl Universe {
    /// Enters `s` into its owner; `s.tree` is `None` for a symbol without TASTy.
    pub fn define(&mut self, s: Symbol) -> SymbolId {
        self.add(s)
    }

    /// The package of qualified name `name`, entered with its enclosing packages if new; `""`
    /// and `<empty>` are the root.
    pub fn enter_package(&mut self, name: &str) -> SymbolId {
        self.package_entered(name)
    }

    /// Sets the declared type `info` returns for `sym`.
    pub fn set_info(&mut self, sym: SymbolId, info: TypeId) {
        self.infos.insert(sym, info);
    }

    /// Sets the parents of class `c`; linearizations computed before are dropped.
    pub fn set_parents(&mut self, c: SymbolId, parents: Vec<TypeId>) {
        self.parents.insert(c, parents);
        self.linearizations.clear();
    }

    pub fn set_flags(&mut self, sym: SymbolId, flags: Flags) {
        self.symbols[sym.0 as usize].flags = flags;
    }

    /// A binder no other type uses, for a type lambda or method type made outside TASTy.
    pub fn fresh_binder(&mut self) -> Binder {
        self.binders += 1;
        Binder(FileId(u32::MAX - 1), self.binders)
    }
}
