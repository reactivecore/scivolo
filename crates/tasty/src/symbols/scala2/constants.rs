// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The constants of a pickle (dotty's `readConstant`).

use super::super::Universe;
use super::Unpickler;
use super::format::*;
use crate::tags::{
    BYTEconst, CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LONGconst, NULLconst, SHORTconst,
    TRUEconst, UNITconst,
};
use crate::types::Type;

impl Unpickler {
    /// The constant type of constant entry `i`; class and enum literals are refused.
    pub fn constant(&mut self, u: &mut Universe, i: usize) -> Result<Type, String> {
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
