// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/serialization/BinaryDeserializer.scala
use super::Reader;
use crate::cursor::Result;
use crate::nir::{Local, Type, TypeId, Val, ValId};
use crate::prelude::Section;
use crate::tags::{ty, val};

impl Reader<'_> {
    pub fn ty(&mut self) -> Result<TypeId> {
        self.interned(Section::Types, |r| {
            let t = match r.tag()? {
                ty::VARARG => Type::Vararg,
                ty::PTR => Type::Ptr,
                ty::BOOL => Type::Bool,
                ty::CHAR => Type::Char,
                ty::BYTE => Type::Byte,
                ty::SHORT => Type::Short,
                ty::INT => Type::Int,
                ty::LONG => Type::Long,
                ty::FLOAT => Type::Float,
                ty::DOUBLE => Type::Double,
                ty::ARRAY_VALUE => Type::ArrayValue(r.ty()?, r.c.uleb32()?),
                ty::STRUCT_VALUE => Type::StructValue(r.types()?),
                ty::FUNCTION => Type::Function(r.types()?, r.ty()?),
                ty::NULL => Type::Null,
                ty::NOTHING => Type::Nothing,
                ty::VIRTUAL => Type::Virtual,
                ty::VAR => Type::Var(r.ty()?),
                ty::UNIT => Type::Unit,
                ty::ARRAY => Type::Array { elem: r.ty()?, nullable: r.c.bool()? },
                ty::REF => Type::Ref { name: r.global()?, exact: r.c.bool()?, nullable: r.c.bool()? },
                ty::SIZE => Type::Size,
                ty::INT128 => Type::Int128,
                t => return r.bad_tag("type", t),
            };
            Ok(r.pool.types.intern(t))
        })
        .map(TypeId)
    }

    pub fn types(&mut self) -> Result<Vec<TypeId>> {
        self.seq(Self::ty)
    }

    pub fn local(&mut self) -> Result<Local> {
        self.c.uleb64().map(Local)
    }

    pub fn val(&mut self) -> Result<ValId> {
        self.interned(Section::Vals, |r| {
            let v = match r.tag()? {
                val::TRUE => Val::True,
                val::FALSE => Val::False,
                val::NULL => Val::Null,
                val::ZERO => Val::Zero(r.ty()?),
                val::BYTE => Val::Byte(r.c.u8()? as i8),
                val::CHAR => Val::Char(r.c.uleb32()? as u16),
                val::SHORT => Val::Short(r.c.sleb32()? as i16),
                val::INT => Val::Int(r.c.sleb32()?),
                val::LONG => Val::Long(r.c.sleb64()?),
                val::FLOAT => Val::Float(r.c.be_u32()?),
                val::DOUBLE => Val::Double(r.c.be_u64()?),
                val::STRUCT_VALUE => Val::StructValue(r.vals()?),
                val::ARRAY_VALUE => Val::ArrayValue(r.ty()?, r.vals()?),
                val::BYTE_STRING => {
                    let n = r.c.len_prefixed()?;
                    Val::ByteString(r.c.bytes(n)?.to_vec())
                }
                val::LOCAL => Val::Local(r.local()?, r.ty()?),
                val::GLOBAL => Val::Global(r.global()?, r.ty()?),
                val::UNIT => Val::Unit,
                val::CONST => Val::Const(r.val()?),
                val::STRING => Val::String(r.string()?),
                val::VIRTUAL => Val::Virtual(r.c.uleb64()?),
                val::CLASS_OF => Val::ClassOf(r.global()?),
                val::SIZE => Val::Size(r.c.uleb64()?),
                val::INT128 => Val::Int128(r.c.sleb64()?, r.c.uleb64()?),
                t => return r.bad_tag("value", t),
            };
            Ok(r.pool.vals.intern(v))
        })
        .map(ValId)
    }

    pub fn vals(&mut self) -> Result<Vec<ValId>> {
        self.seq(Self::val)
    }
}
