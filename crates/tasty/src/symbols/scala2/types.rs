// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The value types of a pickle (dotty's `readType`, `readConstant`): references to classes,
//! type parameters, aliases and objects, applied types, bounds and constants.

use super::super::{Kind, SymbolId, Universe};
use super::format::*;
use super::{Sym, Unpickler};
use crate::tags::{
    BYTEconst, CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LONGconst, NULLconst, OBJECT,
    SHORTconst, TRUEconst, UNITconst,
};
use crate::types::{Type, TypeId};

impl Unpickler {
    /// Reads type entry `i`; refuses refinement, existential, annotated and super types, path
    /// dependent and dependent method types and unapplied type constructors.
    pub fn ty(&mut self, u: &mut Universe, i: usize) -> Result<TypeId, String> {
        let saved = self.buf.at;
        let (tag, end) = self.seek(i)?;
        let t = match tag {
            NOPREFIXtpe => Type::NoPrefix,
            THIStpe => {
                let s = self.known(u)?;
                let class = u.types.intern(Type::Sym(None, s));
                Type::This(class)
            }
            SINGLEtpe => {
                let pre = self.buf.nat()?;
                self.check_prefix(u, pre)?;
                let s = self.known(u)?;
                if u.symbol(s).kind != Kind::Package && !u.symbol(s).flags.has(OBJECT) {
                    return Err("a singleton type of a value".into());
                }
                Type::Sym(None, s)
            }
            CONSTANTtpe => {
                let c = self.buf.nat()?;
                self.constant(u, c)?
            }
            TYPEREFtpe => return self.type_ref(u, end).inspect(|_| self.buf.at = saved),
            TYPEBOUNDStpe => {
                let (lo, hi) = (self.buf.nat()?, self.buf.nat()?);
                Type::Bounds(self.ty(u, lo)?, self.ty(u, hi)?)
            }
            _ => return Err(refused_type(tag)),
        };
        self.buf.at = saved;
        Ok(u.types.intern(t))
    }

    /// Reads the symbol the buffer refers to next, one of the universe.
    fn known(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        let r = self.buf.nat()?;
        match self.sym(u, r)? {
            Sym::Known(s) => Ok(s),
            Sym::Param => Err("a dependent method type".into()),
            _ => Err("a reference to a symbol not read".into()),
        }
    }

    /// Reads a `TYPEREFtpe` body ending at `end`: a type parameter of a method in scope, or a
    /// class, alias or class type parameter, applied to the arguments that follow; an alias is
    /// followed.
    fn type_ref(&mut self, u: &mut Universe, end: usize) -> Result<TypeId, String> {
        let pre = self.buf.nat()?;
        let sym = self.buf.nat()?;
        let mut args = Vec::new();
        while self.buf.at < end {
            args.push(self.buf.nat()?);
        }
        if let Some(&t) = self.params.get(&sym) {
            return if args.is_empty() { Ok(t) } else { Err("a higher-kinded type parameter".into()) };
        }
        self.check_prefix(u, pre)?;
        self.buf.at = end;
        let s = match self.sym(u, sym)? {
            Sym::Known(s) => s,
            Sym::Param => return Err("a dependent method type".into()),
            _ => return Err("a reference to a symbol not read".into()),
        };
        if !matches!(u.symbol(s).kind, Kind::Class | Kind::Type | Kind::TypeParam) {
            return Err(format!("a type reference to term `{}`", u.full_name(s)));
        }
        let args = args.into_iter().map(|a| self.ty(u, a)).collect::<Result<Vec<_>, _>>()?;
        let tycon = u.types.intern(Type::Sym(None, s));
        let t = if args.is_empty() { tycon } else { u.types.intern_applied(tycon, &args) };
        if u.symbol(s).kind == Kind::Type {
            let dealiased = u.dealias(t);
            return match dealiased != t || args.is_empty() {
                true => Ok(dealiased),
                false => Err(format!("`{}` applied to type arguments", u.full_name(s))),
            };
        }
        let arity = u.type_params(s).len();
        if arity != args.len() {
            return Err(format!(
                "`{}` applied to {} of its {arity} type arguments",
                u.full_name(s),
                args.len()
            ));
        }
        Ok(t)
    }

    /// Refuses prefix entry `pre` unless it is static: none, a package's or class's `this`, or
    /// an object.
    fn check_prefix(&mut self, u: &mut Universe, pre: usize) -> Result<(), String> {
        let saved = self.buf.at;
        let (tag, _) = self.seek(pre)?;
        let known = match tag {
            NOPREFIXtpe => None,
            THIStpe | SINGLEtpe => {
                if tag == SINGLEtpe {
                    self.buf.nat()?;
                }
                Some(self.known(u)?)
            }
            _ => return Err(format!("a type selected from {}", refused_type(tag))),
        };
        self.buf.at = saved;
        match known {
            Some(s)
                if tag == SINGLEtpe
                    && !u.symbol(s).flags.has(OBJECT)
                    && u.symbol(s).kind != Kind::Package =>
            {
                Err("a path-dependent type".into())
            }
            _ => Ok(()),
        }
    }

    /// The constant type of constant entry `i`; class and enum literals are refused.
    fn constant(&mut self, u: &mut Universe, i: usize) -> Result<Type, String> {
        let saved = self.buf.at;
        let (tag, end) = self.seek(i)?;
        let len = end - self.buf.at;
        let t = match tag {
            LITERALunit => Type::Constant(UNITconst, 0),
            LITERALnull => Type::Constant(NULLconst, 0),
            LITERALboolean => {
                Type::Constant(if self.buf.long(len)? != 0 { TRUEconst } else { FALSEconst }, 0)
            }
            LITERALbyte => Type::Constant(BYTEconst, i64::from(self.buf.long(len)? as i8)),
            LITERALshort => Type::Constant(SHORTconst, i64::from(self.buf.long(len)? as i16)),
            LITERALchar => Type::Constant(CHARconst, i64::from(self.buf.long(len)? as u16)),
            LITERALint => Type::Constant(INTconst, i64::from(self.buf.long(len)? as i32)),
            LITERALlong => Type::Constant(LONGconst, self.buf.long(len)?),
            LITERALfloat => Type::Constant(FLOATconst, i64::from(self.buf.long(len)? as u32)),
            LITERALdouble => Type::Constant(DOUBLEconst, self.buf.long(len)?),
            LITERALstring => {
                let name = self.buf.nat()?;
                Type::Str(u.types.intern_name(&self.name(name)?.0))
            }
            _ => return Err(format!("a constant of tag {tag}")),
        };
        self.buf.at = saved;
        Ok(t)
    }
}
