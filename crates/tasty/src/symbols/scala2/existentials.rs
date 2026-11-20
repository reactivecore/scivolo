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
        let error = u.types.intern(Type::Error);
        let saved: Vec<_> = bound.iter().map(|&b| (b, self.existentials.insert(b, error))).collect();
        let read = self.bound_infos(u, &bound).and_then(|bounds| {
            bound.iter().zip(bounds).for_each(|(&b, info)| _ = self.existentials.insert(b, info));
            self.ty(u, res)
        });
        for (b, before) in saved {
            match before {
                Some(t) => self.existentials.insert(b, t),
                None => self.existentials.remove(&b),
            };
        }
        read
    }

    /// The bounds of bound symbol entries `bound`, refusing those of a higher-kinded one.
    fn bound_infos(&mut self, u: &mut Universe, bound: &[usize]) -> Result<Vec<TypeId>, String> {
        let mut bounds = Vec::new();
        for &b in bound {
            let info = self.symbol_info(u, b)?;
            if !matches!(u.ty(info), Type::Bounds(..)) {
                return Err("an existential type of a higher-kinded type".into());
            }
            bounds.push(info);
        }
        Ok(bounds)
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
            let read = self.existentials.get(&sym).filter(|&&b| !applied && !matches!(u.ty(b), Type::Error));
            if let Some(&bounds) = read {
                return Ok(bounds);
            }
        }
        self.ty(u, i)
    }

    /// Dealiases alias `tycon` applied to `args`, some of them wildcards, as dotty's
    /// `elimExistentials` does: the alias applied to the bound symbols, then those that are type
    /// arguments of the result replaced by their bounds. Refuses a bound symbol the alias puts
    /// elsewhere or more than once.
    pub fn wildcard_alias(
        &mut self,
        u: &mut Universe,
        tycon: TypeId,
        args: &[TypeId],
    ) -> Result<TypeId, String> {
        let binder = u.fresh_binder();
        let markers: Vec<TypeId> =
            (0..args.len()).map(|k| u.types.intern(Type::Param(binder, k as u32))).collect();
        let applied = u.types.intern_applied(tycon, &markers);
        let dealiased = u.dealias(applied);
        let top: Vec<TypeId> = u.applied(dealiased).map(|(_, a)| a.to_vec()).unwrap_or_default();
        for (k, &arg) in args.iter().enumerate() {
            if u.bounds(arg).is_none() {
                continue;
            }
            let mut count = 0;
            u.map_type(dealiased, &mut |_, t| {
                count += usize::from(t == markers[k]);
                None
            });
            if count != 1 || !top.contains(&markers[k]) {
                return Err("an existential type that is no wildcard".into());
            }
        }
        Ok(u.map_type(dealiased, &mut |_, t| markers.iter().position(|&m| m == t).map(|k| args[k])))
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
