// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! `SHAREDtype`: the types pickled before, keyed by what tells dotty's types apart.

use super::{Refusal, TreePickler};
use tasty::tags::*;
use tasty::write::Addr;
use tasty::{SymbolId, Type, TypeId};

/// The key byte of a reference by name, told from a reference to a symbol (dotty's designators).
pub const BY_NAME: u8 = 0;

impl TreePickler<'_> {
    /// Type `t`, a `SHAREDtype` if pickled before; `rich` admits method types.
    pub fn ty(&mut self, t: TypeId, rich: bool) -> Result<(), Refusal> {
        if let Type::Skolem(_, info) = *self.u.ty(t) {
            return self.ty(info, rich);
        }
        let at = self.buf.buf.current_addr();
        if !self.keying {
            let key = self.key(t, rich)?;
            if self.is_shared(key) {
                return Ok(());
            }
        }
        self.new_type(t, rich, at)
    }

    /// What identifies type `t` as dotty shares types: its bytes without sharing, the `this` of a
    /// package told from the package's reference, definitions and names by identity.
    fn key(&mut self, t: TypeId, rich: bool) -> Result<Vec<u8>, Refusal> {
        if let Some(k) = self.keys.get(&t) {
            return Ok(k.clone());
        }
        let (buf, binders) = (std::mem::take(&mut self.buf), self.binders.clone());
        self.keying = true;
        let pickled = self.new_type(t, rich, Addr(0));
        self.keying = false;
        let scratch = std::mem::replace(&mut self.buf, buf);
        self.binders = binders;
        pickled?;
        self.keys.insert(t, scratch.buf.bytes.clone());
        Ok(scratch.buf.bytes)
    }

    /// Makes later occurrences of type `t` share the tree at the current address.
    pub fn share_here(&mut self, t: TypeId) -> Result<(), Refusal> {
        let key = self.key(t, false)?;
        self.types.insert(key, self.buf.buf.current_addr());
        Ok(())
    }

    /// The `this` of package `p`, a `SHAREDtype` if pickled before (dotty's `ThisType` case).
    pub fn package_this(&mut self, p: SymbolId) {
        if self.keying || !self.is_shared(this_key(p)) {
            self.new_package_this(p);
        }
    }

    /// The `this` of package `p`, not shared.
    pub fn new_package_this(&mut self, p: SymbolId) {
        if self.keying {
            return self.buf.buf.write_bytes(&this_key(p));
        }
        if p == self.u.root() {
            self.byte(THIS);
            self.byte(TYPEREFpkg);
        } else {
            self.byte(TERMREFpkg);
        }
        self.package_name(p);
    }

    /// Writes a `SHAREDtype` if `key` was pickled before, else records it at the current address.
    fn is_shared(&mut self, key: Vec<u8>) -> bool {
        match self.types.get(&key) {
            Some(&at) => {
                self.byte(SHAREDtype);
                self.buf.write_ref(at);
                true
            }
            None => {
                self.types.insert(key, self.buf.buf.current_addr());
                false
            }
        }
    }
}

/// What identifies the `this` of package `p`, told from the package's reference.
fn this_key(p: SymbolId) -> Vec<u8> {
    [&[THIS, TERMREFpkg][..], &p.0.to_le_bytes()].concat()
}
