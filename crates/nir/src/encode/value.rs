// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/serialization/BinarySerializer.scala
use super::{Out, Writer};
use crate::nir::{Local, Type, TypeId, Val, ValId};
use crate::prelude::Section;
use crate::tags::{ty, val};

impl Writer<'_> {
    pub fn ty(&mut self, out: &mut Out, id: TypeId) {
        let off = self.interned(Section::Types, id.0, |w, e| match w.pool.ty(id).clone() {
            Type::Vararg => e.u8(ty::VARARG),
            Type::Ptr => e.u8(ty::PTR),
            Type::Bool => e.u8(ty::BOOL),
            Type::Char => e.u8(ty::CHAR),
            Type::Byte => e.u8(ty::BYTE),
            Type::Short => e.u8(ty::SHORT),
            Type::Int => e.u8(ty::INT),
            Type::Long => e.u8(ty::LONG),
            Type::Float => e.u8(ty::FLOAT),
            Type::Double => e.u8(ty::DOUBLE),
            Type::Size => e.u8(ty::SIZE),
            Type::Int128 => e.u8(ty::INT128),
            Type::ArrayValue(elem, n) => {
                e.u8(ty::ARRAY_VALUE);
                w.ty(e, elem);
                e.uleb(n as u64);
            }
            Type::StructValue(tys) => {
                e.u8(ty::STRUCT_VALUE);
                w.types(e, &tys);
            }
            Type::Function(args, ret) => {
                e.u8(ty::FUNCTION);
                w.types(e, &args);
                w.ty(e, ret);
            }
            Type::Null => e.u8(ty::NULL),
            Type::Nothing => e.u8(ty::NOTHING),
            Type::Virtual => e.u8(ty::VIRTUAL),
            Type::Var(t) => {
                e.u8(ty::VAR);
                w.ty(e, t);
            }
            Type::Unit => e.u8(ty::UNIT),
            Type::Array { elem, nullable } => {
                e.u8(ty::ARRAY);
                w.ty(e, elem);
                e.bool(nullable);
            }
            Type::Ref { name, exact, nullable } => {
                e.u8(ty::REF);
                w.global(e, name);
                e.bool(exact);
                e.bool(nullable);
            }
        });
        out.uleb(off as u64);
    }

    pub fn types(&mut self, out: &mut Out, tys: &[TypeId]) {
        out.uleb(tys.len() as u64);
        tys.iter().for_each(|&t| self.ty(out, t));
    }

    pub fn local(&mut self, out: &mut Out, l: Local) {
        out.uleb(l.0);
    }

    pub fn val(&mut self, out: &mut Out, id: ValId) {
        let off = self.interned(Section::Vals, id.0, |w, e| match w.pool.val(id).clone() {
            Val::True => e.u8(val::TRUE),
            Val::False => e.u8(val::FALSE),
            Val::Null => e.u8(val::NULL),
            Val::Unit => e.u8(val::UNIT),
            Val::Zero(t) => {
                e.u8(val::ZERO);
                w.ty(e, t);
            }
            Val::Byte(b) => {
                e.u8(val::BYTE);
                e.u8(b as u8);
            }
            Val::Char(c) => {
                e.u8(val::CHAR);
                e.uleb(c as u64);
            }
            Val::Short(s) => {
                e.u8(val::SHORT);
                e.sleb(s as i64);
            }
            Val::Int(i) => {
                e.u8(val::INT);
                e.sleb(i as i64);
            }
            Val::Long(l) => {
                e.u8(val::LONG);
                e.sleb(l);
            }
            Val::Float(bits) => {
                e.u8(val::FLOAT);
                e.be_u32(bits);
            }
            Val::Double(bits) => {
                e.u8(val::DOUBLE);
                e.be_u64(bits);
            }
            Val::Size(s) => {
                e.u8(val::SIZE);
                e.uleb(s);
            }
            Val::Int128(hi, lo) => {
                e.u8(val::INT128);
                e.sleb(hi);
                e.uleb(lo);
            }
            Val::StructValue(vals) => {
                e.u8(val::STRUCT_VALUE);
                w.vals(e, &vals);
            }
            Val::ArrayValue(t, vals) => {
                e.u8(val::ARRAY_VALUE);
                w.ty(e, t);
                w.vals(e, &vals);
            }
            Val::ByteString(bytes) => {
                e.u8(val::BYTE_STRING);
                e.uleb(bytes.len() as u64);
                e.0.extend_from_slice(&bytes);
            }
            Val::Local(l, t) => {
                e.u8(val::LOCAL);
                w.local(e, l);
                w.ty(e, t);
            }
            Val::Global(g, t) => {
                e.u8(val::GLOBAL);
                w.global(e, g);
                w.ty(e, t);
            }
            Val::Const(v) => {
                e.u8(val::CONST);
                w.val(e, v);
            }
            Val::String(s) => {
                e.u8(val::STRING);
                w.string(e, s);
            }
            Val::Virtual(v) => {
                e.u8(val::VIRTUAL);
                e.uleb(v);
            }
            Val::ClassOf(g) => {
                e.u8(val::CLASS_OF);
                w.global(e, g);
            }
        });
        out.uleb(off as u64);
    }

    pub fn vals(&mut self, out: &mut Out, vals: &[ValId]) {
        out.uleb(vals.len() as u64);
        vals.iter().for_each(|&v| self.val(out, v));
    }
}
