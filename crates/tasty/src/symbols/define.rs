//! Symbols defined outside TASTy (a script's), what completes them (infos, parents, flags),
//! and binders for the types made with them.

use super::{FileId, Flags, Kind, Symbol, SymbolId, Universe};
use crate::Erased;
use crate::types::{Binder, TypeId};

impl Universe {
    /// Enters `s` into its owner; `s.tree` is `None` for a symbol without TASTy.
    /// Resolved references are dropped when an owner already looked up by name gets a name new
    /// to it, a class member or a symbol without TASTy.
    pub fn define(&mut self, s: Symbol) -> SymbolId {
        let id = SymbolId(self.symbols.len() as u32);
        if let Some(o) = s.owner {
            self.symbols[o.0 as usize].decls.push(id);
            if let Some(index) = self.by_name.get_mut(&o) {
                let named = index.entry(s.name.clone()).or_default();
                let new_name = named.is_empty();
                named.push(id);
                if new_name || s.tree.is_none() || self.symbols[o.0 as usize].kind == Kind::Class {
                    self.changed();
                }
            }
        }
        if let Some(tree) = s.tree {
            self.by_tree.insert(tree, id);
        }
        self.symbols.push(s);
        id
    }

    /// Drops the name index of `owner` and what `changed` drops.
    fn decls_changed(&mut self, owner: SymbolId) {
        self.by_name.remove(&owner);
        self.changed();
    }

    /// Drops the resolved references and the erased methods, and starts a new generation.
    fn changed(&mut self) {
        self.resolved.clear();
        self.erased_methods.clear();
        self.generation += 1;
    }

    /// How often declarations, parents, or set infos and flags changed; results derived from
    /// the universe stay valid while it is the same.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// How many binders `fresh_binder` has made.
    pub fn binders_made(&self) -> u32 {
        self.binders
    }

    /// The erased parameters and result of method `def` remembered since the last change, with
    /// value classes semi-erased if `semi`.
    pub fn erased_memo(&self, def: SymbolId, semi: bool) -> Option<&(Vec<Erased>, Erased)> {
        self.erased_methods.get(&(def, semi))
    }

    /// Remembers `erased` as `erased_memo`'s answer if the generation is still `generation`.
    pub fn remember_erased(
        &mut self,
        (def, semi): (SymbolId, bool),
        generation: u64,
        erased: &(Vec<Erased>, Erased),
    ) {
        if self.generation == generation {
            self.erased_methods.insert((def, semi), erased.clone());
        }
    }

    /// The package of qualified name `name`, entered with its enclosing packages if new; `""`
    /// and `<empty>` are the root.
    pub fn enter_package(&mut self, name: &str) -> SymbolId {
        self.package_entered(name)
    }

    /// Sets the declared type `info` returns for `sym`; replacing a different info drops the
    /// resolved references.
    pub fn set_info(&mut self, sym: SymbolId, info: TypeId) {
        if self.infos.insert(sym, info).is_some_and(|old| old != info) {
            self.changed();
        }
    }

    /// Sets the parents of class `c`; linearizations and resolved references are dropped.
    pub fn set_parents(&mut self, c: SymbolId, parents: Vec<TypeId>) {
        self.parents.insert(c, parents);
        self.linearizations.clear();
        self.changed();
    }

    /// Sets the flags of `sym`; changed flags drop the resolved references.
    pub fn set_flags(&mut self, sym: SymbolId, flags: Flags) {
        if std::mem::replace(&mut self.symbols[sym.0 as usize].flags, flags) != flags {
            self.changed();
        }
    }

    /// Gives `sym`, a symbol defined outside TASTy, the run-time name `target` (`@targetName`).
    pub fn set_target(&mut self, sym: SymbolId, target: &str) {
        self.symbols[sym.0 as usize].target = Some(target.to_owned());
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
            self.decls_changed(owner);
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
            self.decls_changed(owner);
        }
    }

    /// Renames `sym`, a symbol defined outside TASTy.
    pub fn rename(&mut self, sym: SymbolId, name: &str) {
        self.symbols[sym.0 as usize].name = name.to_owned();
        if let Some(owner) = self.symbol(sym).owner {
            self.decls_changed(owner);
        }
    }

    /// A binder no other type uses, for a type lambda or method type made outside TASTy.
    pub fn fresh_binder(&mut self) -> Binder {
        self.binders += 1;
        Binder(FileId(u32::MAX - 1), self.binders)
    }

    /// A skolem of type `info`, different from every other.
    pub fn fresh_skolem(&mut self, info: TypeId) -> TypeId {
        let b = self.fresh_binder();
        self.intern_type(crate::Type::Skolem(b, info))
    }
}
