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

    /// Records whether `sym`, a term defined outside TASTy, is abstract.
    pub fn set_deferred(&mut self, sym: SymbolId, deferred: bool) {
        if deferred {
            self.deferred.insert(sym);
        } else {
            self.deferred.remove(&sym);
        }
    }

    /// Records that class `c`, defined outside TASTy, has initialization code.
    pub fn set_inits(&mut self, c: SymbolId) {
        self.with_inits.insert(c);
    }

    /// Whether `sym` is a method or value defined outside TASTy without a body; `false` for
    /// the library's symbols, whose bodies `body` gives.
    pub fn is_deferred(&self, sym: SymbolId) -> bool {
        self.deferred.contains(&sym)
    }

    /// Removes `sym`, a symbol defined outside TASTy, from the declarations of its owner.
    pub fn unenter(&mut self, sym: SymbolId) {
        if let Some(owner) = self.symbol(sym).owner {
            self.symbols[owner.0 as usize].decls.retain(|&d| d != sym);
        }
    }

    /// Makes `owner` the owner of `sym`, a local symbol defined outside TASTy: no longer a
    /// declaration of its former owner.
    pub fn set_owner(&mut self, sym: SymbolId, owner: SymbolId) {
        self.unenter(sym);
        self.symbols[sym.0 as usize].owner = Some(owner);
    }

    /// Enters `sym`, a symbol defined outside TASTy, into the declarations of its owner.
    pub fn enter(&mut self, sym: SymbolId) {
        if let Some(owner) = self.symbol(sym).owner {
            self.symbols[owner.0 as usize].decls.push(sym);
        }
    }

    /// Renames `sym`, a symbol defined outside TASTy.
    pub fn rename(&mut self, sym: SymbolId, name: &str) {
        self.symbols[sym.0 as usize].name = name.to_owned();
    }

    /// A binder no other type uses, for a type lambda or method type made outside TASTy.
    pub fn fresh_binder(&mut self) -> Binder {
        self.binders += 1;
        Binder(FileId(u32::MAX - 1), self.binders)
    }
}
