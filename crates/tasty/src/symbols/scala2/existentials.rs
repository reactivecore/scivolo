// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! Existential types of a pickle (dotty's `elimExistentials`): `C[T] forSome { type T >: L <: H }`
//! read as `C[? >: L <: H]`.

use super::super::Universe;
use super::Unpickler;
use super::format::TYPEREFtpe;
use crate::types::{Type, TypeId};

impl Unpickler {
    /// Reads an `EXISTENTIALtpe` body ending at `end`: its result with each type argument that
    /// is a bound symbol replaced by the symbol's bounds. Refuses another occurrence of a bound
    /// symbol, where dotty substitutes bounds and warns (`FailureToEliminateExistential`), bounds
    /// that refer to a bound symbol, and the bounds of a higher-kinded one.
    pub fn existential(&mut self, u: &mut Universe, end: usize) -> Result<TypeId, String> {
        let res = self.buf.nat()?;
        let bound = self.refs(end)?;
        let mut bounds = Vec::new();
        for &b in &bound {
            let info = self.symbol_info(u, b)?;
            if !matches!(u.ty(info), Type::Bounds(..)) {
                return Err("an existential type of a higher-kinded type".into());
            }
            bounds.push(info);
        }
        let saved: Vec<_> =
            bound.iter().zip(&bounds).map(|(&b, &info)| (b, self.existentials.insert(b, info))).collect();
        let read = self.ty(u, res);
        for (b, before) in saved {
            match before {
                Some(t) => self.existentials.insert(b, t),
                None => self.existentials.remove(&b),
            };
        }
        read
    }

    /// Reads type argument entry `i`: the bounds of a bound symbol of an enclosing existential
    /// type (dotty's `mapArg`), else the type.
    pub fn type_arg(&mut self, u: &mut Universe, i: usize) -> Result<TypeId, String> {
        if self.tag(i)? == TYPEREFtpe {
            let saved = self.buf.at;
            let (_, end) = self.seek(i)?;
            self.buf.nat()?;
            let sym = self.buf.nat()?;
            let applied = self.buf.at < end;
            self.buf.at = saved;
            if let Some(&bounds) = self.existentials.get(&sym).filter(|_| !applied) {
                return Ok(bounds);
            }
        }
        self.ty(u, i)
    }

    /// Refuses a reference to symbol entry `sym` if it is bound by an enclosing existential type:
    /// it is no type argument dotty's `elimExistentials` replaces.
    pub fn check_not_bound(&self, sym: usize) -> Result<(), String> {
        match self.existentials.contains_key(&sym) {
            true => Err("an existential type that is no wildcard".into()),
            false => Ok(()),
        }
    }
}
