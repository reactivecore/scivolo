// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala

use super::super::Universe;
use crate::tags::{
    BYTEconst, CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LONGconst, SHORTconst, TRUEconst,
};
use crate::types::{Type, TypeId};
use classfile::ConstantValue;

/// The constant type of a field of descriptor `descriptor` and `ConstantValue` `c`, its `Integer`
/// entry converted to the field's type (dotty's `convertTo`).
pub fn constant_type(u: &mut Universe, descriptor: &str, c: &ConstantValue) -> Result<TypeId, String> {
    let t = match (descriptor, c) {
        ("Z", &ConstantValue::Int(i)) => Type::Constant(if i != 0 { TRUEconst } else { FALSEconst }, 0),
        ("B", &ConstantValue::Int(i)) => Type::Constant(BYTEconst, i64::from(i as i8)),
        ("S", &ConstantValue::Int(i)) => Type::Constant(SHORTconst, i64::from(i as i16)),
        ("C", &ConstantValue::Int(i)) => Type::Constant(CHARconst, i64::from(i as u16)),
        ("I", &ConstantValue::Int(i)) => Type::Constant(INTconst, i64::from(i)),
        ("J", &ConstantValue::Long(l)) => Type::Constant(LONGconst, l),
        ("F", &ConstantValue::Float(f)) => Type::Constant(FLOATconst, i64::from(f)),
        ("D", &ConstantValue::Double(d)) => Type::Constant(DOUBLEconst, d as i64),
        ("Ljava/lang/String;", ConstantValue::String(s)) => Type::Str(u.types.intern_name(s)),
        _ => return Err(format!("constant {c:?} of a field of type {descriptor}")),
    };
    Ok(u.types.intern(t))
}
