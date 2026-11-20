// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The types of the members of a pickle (dotty's `LocalUnpickler.complete`, the `METHODtpe` and
//! `POLYtpe` cases of `readType`, `addConstructorTypeParams`).

use super::super::encoded_names::decode;
use super::super::{Kind, SymbolId, Universe};
use super::format::{IMPLICITMETHODtpe, METHODtpe, POLYtpe};
use super::{Sym, Unpickler, flags as f};
use crate::tags::OBJECT;
use crate::types::{NameId, Type, TypeId};
use names::nme;

impl Unpickler {
    /// Gives member `s` of entry `i` its type: a getter its result as a value, a method without
    /// parameter lists its result, a constructor its class's type parameters as its own and its
    /// class applied to them as result.
    pub fn member_info(&mut self, u: &mut Universe, i: usize, s: SymbolId) -> Result<(), String> {
        let h = self.header(i)?;
        let info = match u.symbol(s).kind {
            Kind::Val if u.symbol(s).flags.has(OBJECT) => self.ty(u, h.info)?,
            Kind::Val => self.value(u, h.info)?,
            _ if u.symbol(s).name == nme::CONSTRUCTOR => self.constructor(u, s, h.info)?,
            _ => self.method(u, h.info, None)?,
        };
        u.infos.insert(s, info);
        Ok(())
    }

    /// The type of a getter of type entry `i`: the result of a `POLYtpe` without type
    /// parameters (an `ExprType`), else the type.
    fn value(&mut self, u: &mut Universe, i: usize) -> Result<TypeId, String> {
        let saved = self.buf.at;
        let (tag, end) = self.seek(i)?;
        let result = self.buf.nat()?;
        let expr = tag == POLYtpe && self.buf.at == end;
        self.buf.at = saved;
        if expr { self.ty(u, result) } else { self.ty(u, i) }
    }

    /// The type of a constructor of class `s`'s owner from type entry `i`: polymorphic in the
    /// class's type parameters, returning the class applied to them.
    fn constructor(&mut self, u: &mut Universe, s: SymbolId, i: usize) -> Result<TypeId, String> {
        let class = u.symbol(s).owner.expect("a constructor has an owner");
        let params: Vec<(usize, SymbolId)> = self
            .entered
            .iter()
            .filter(|&&(_, p)| u.symbol(p).owner == Some(class) && u.symbol(p).kind == Kind::TypeParam)
            .copied()
            .collect();
        let class_type = u.types.intern(Type::Sym(None, class));
        if params.is_empty() {
            return self.method(u, i, Some(class_type));
        }
        let binder = u.fresh_binder();
        for (k, &(p, _)) in params.iter().enumerate() {
            let param = u.types.intern(Type::Param(binder, k as u32));
            self.params.insert(p, param);
        }
        let args: Vec<TypeId> = params.iter().map(|&(p, _)| self.params[&p]).collect();
        let result = u.types.intern_applied(class_type, &args);
        let entries: Vec<usize> = params.iter().map(|&(p, _)| p).collect();
        let typed = self.poly(u, binder, &entries, |up, u| up.method(u, i, Some(result)));
        params.iter().for_each(|(p, _)| _ = self.params.remove(p));
        typed
    }

    /// The type of method type entry `i`: a `POLYtpe` with type parameters a polymorphic type,
    /// without the type of its result, a `METHODtpe` a method type, its last result replaced
    /// by `result` if given.
    fn method(&mut self, u: &mut Universe, i: usize, result: Option<TypeId>) -> Result<TypeId, String> {
        let saved = self.buf.at;
        let (tag, end) = self.seek(i)?;
        let res = self.buf.nat()?;
        let mut refs = Vec::new();
        while self.buf.at < end {
            refs.push(self.buf.nat()?);
        }
        self.buf.at = saved;
        match tag {
            POLYtpe if refs.is_empty() => result.map_or_else(|| self.ty(u, res), Ok),
            POLYtpe => {
                let binder = u.fresh_binder();
                for (k, &p) in refs.iter().enumerate() {
                    let param = u.types.intern(Type::Param(binder, k as u32));
                    self.params.insert(p, param);
                }
                let typed = self.poly(u, binder, &refs, |up, u| up.method(u, res, result));
                refs.iter().for_each(|p| _ = self.params.remove(p));
                typed
            }
            METHODtpe | IMPLICITMETHODtpe => self.method_type(u, tag, res, &refs, result),
            _ if result.is_some() => Err(format!("a constructor of type tag {tag}")),
            _ => self.ty(u, i),
        }
    }

    /// The polymorphic type of binder `binder` over type parameter entries `params`, in scope,
    /// with their bounds, and the result `result` reads.
    fn poly(
        &mut self,
        u: &mut Universe,
        binder: crate::Binder,
        params: &[usize],
        result: impl FnOnce(&mut Self, &mut Universe) -> Result<TypeId, String>,
    ) -> Result<TypeId, String> {
        let mut bounds: Vec<(NameId, TypeId)> = Vec::new();
        for &p in params {
            let h = self.header(p)?;
            let b = self.ty(u, h.info)?;
            bounds.push((u.types.intern_name(&decode(&h.name)), b));
        }
        let result = result(self, u)?;
        Ok(u.types.intern_poly(binder, &bounds, result))
    }

    /// The method type of tag `tag`, result entry `res` and parameter entries `refs`, implicit
    /// if its first parameter is; its last result replaced by `result` if given.
    fn method_type(
        &mut self,
        u: &mut Universe,
        tag: u8,
        res: usize,
        refs: &[usize],
        result: Option<TypeId>,
    ) -> Result<TypeId, String> {
        let mut params = Vec::new();
        let mut implicit = tag == IMPLICITMETHODtpe;
        for (k, &p) in refs.iter().enumerate() {
            if self.sym(u, p)? != Sym::Param {
                return Err("a parameter of another symbol".into());
            }
            let h = self.header(p)?;
            implicit |= k == 0 && h.flags & f::IMPLICIT != 0;
            let t = self.ty(u, h.info)?;
            params.push((u.types.intern_name(&decode(&h.name)), t));
        }
        let result = match (self.tag(res)?, result) {
            (METHODtpe | IMPLICITMETHODtpe, _) => self.method(u, res, result)?,
            (_, Some(r)) => r,
            (_, None) => self.ty(u, res)?,
        };
        let binder = u.fresh_binder();
        Ok(u.types.intern_method(binder, &params, result, implicit, false))
    }
}
