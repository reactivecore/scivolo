// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The type parameters and parents of the classes of a pickle (dotty's `ClassUnpickler`,
//! `setClassInfo`).

use super::super::builtin::synthetic;
use super::super::encoded_names::decode;
use super::super::{Kind, SymbolId, Universe};
use super::format::{CLASSINFOtpe, POLYtpe};
use super::symbols::Header;
use super::{Sym, Unpickler, flags as f};
use crate::tags::{CONTRAVARIANT, COVARIANT, LOCAL, PRIVATE};
use crate::types::Type;

impl Unpickler {
    /// Enters the type parameters of class `class` of header `h`, those its type lists (dotty's
    /// `ClassUnpickler.init`).
    pub fn type_params(&mut self, u: &mut Universe, h: &Header, class: SymbolId) -> Result<(), String> {
        for p in self.poly_params(h.info)? {
            let ph = self.header(p)?;
            let mut flags = vec![PRIVATE, LOCAL];
            flags.extend((ph.flags & f::COVARIANT != 0).then_some(COVARIANT));
            flags.extend((ph.flags & f::CONTRAVARIANT != 0).then_some(CONTRAVARIANT));
            let s = synthetic(u, class, &decode(&ph.name), Kind::TypeParam, &flags);
            self.entered.push((p, s));
            self.own.insert(s);
            self.syms.insert(p, Sym::Known(s));
        }
        Ok(())
    }

    /// The type parameter entries of type entry `i` if it is a `POLYtpe`, else none.
    pub fn poly_params(&mut self, i: usize) -> Result<Vec<usize>, String> {
        let saved = self.buf.at;
        let (tag, end) = self.seek(i)?;
        let mut params = Vec::new();
        if tag == POLYtpe {
            self.buf.nat()?;
            while self.buf.at < end {
                params.push(self.buf.nat()?);
            }
        }
        self.buf.at = saved;
        Ok(params)
    }

    /// Gives class `s` of entry `i` the bounds of its type parameters and its parents, `Object`
    /// for none and aliases followed (dotty's `setClassInfo`); marks it as Scala 2's, and a class
    /// other than a trait as initializing (dotty sets `NoInits` on Scala 2 traits only).
    pub fn class_info(&mut self, u: &mut Universe, i: usize, s: SymbolId) -> Result<(), String> {
        let h = self.header(i)?;
        let mut info = h.info;
        let (tag, _) = self.seek(info)?;
        if tag == POLYtpe {
            info = self.buf.nat()?;
            for p in self.poly_params(h.info)? {
                let Some(Sym::Known(param)) = self.syms.get(&p).copied() else { continue };
                let bounds = self.symbol_info(u, p)?;
                u.infos.insert(param, bounds);
            }
        }
        let (tag, end) = self.seek(info)?;
        if tag != CLASSINFOtpe {
            return Err(format!("a class of type tag {tag}"));
        }
        self.buf.nat()?;
        let mut refs = Vec::new();
        while self.buf.at < end {
            refs.push(self.buf.nat()?);
        }
        let mut parents = Vec::new();
        for r in refs {
            let t = self.ty(u, r)?;
            parents.push(u.dealias(t));
        }
        if parents.is_empty() {
            parents.push(u.types.intern(Type::Sym(None, u.defn().object)));
        }
        u.parents.insert(s, parents);
        u.set_scala2(s);
        if !u.is_trait(s) {
            u.set_inits(s);
        }
        Ok(())
    }

    /// The type of symbol entry `i`, as its header refers to it.
    pub fn symbol_info(&mut self, u: &mut Universe, i: usize) -> Result<crate::TypeId, String> {
        let h = self.header(i)?;
        self.ty(u, h.info)
    }
}
