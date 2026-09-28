//! `box` and `unbox`, which call Scala Native's box helpers; unboxing a `java.lang` box reads
//! its value field directly.

use crate::interp::code::Mem;
use crate::interp::memory::load_mem;
use crate::interp::{Flow, FuncId};
use crate::{Vm, VmError, heap};
use nir::{Type, TypeId};
use std::cell::Cell;

/// The mangled signature of `equals(Object, Object): Boolean`, without its scope suffix.
const BOXED_EQUALS: &str = "D6equalsL16java.lang.ObjectL16java.lang.ObjectzE";

/// Whether `g` is `BoxesRunTime.equals(Object, Object)`, static or on the module.
pub fn is_boxed_equals(pool: &nir::Pool, g: nir::GlobalId) -> bool {
    matches!(pool.top_name(g), "scala.runtime.BoxesRunTime" | "scala.runtime.BoxesRunTime$")
        && pool.member_sig(g).is_some_and(|s| s.starts_with(BOXED_EQUALS))
}

/// How the value in a `java.lang` box compares: by its bits, or as a floating-point number.
#[derive(Clone, Copy)]
pub enum BoxValue {
    Bits(Mem),
    F32,
    F64,
}

impl Vm {
    /// Boxes or unboxes `v`, caching in `cells` the helper's module and function and, for
    /// `java.lang` boxes, the box class's type object and the value field's offset and width.
    pub fn boxed(&mut self, ty: TypeId, boxing: bool, v: u64, cells: &[Cell<u64>]) -> Flow {
        if cells[0].get() == 0 {
            let (module, f) = self.box_helper(ty, boxing)?;
            cells[1].set(f as u64);
            cells[0].set(module);
            if !boxing && let Some((rtti, field)) = self.value_field(ty)? {
                cells[2].set(rtti as u64);
                cells[3].set(field);
            }
        }
        if cells[2].get() != 0 {
            if v == 0 {
                return Ok(0);
            }
            if heap::read::<u64>(v as usize) == cells[2].get() {
                let (offset, width) = (cells[3].get() >> 8, cells[3].get() & 0xFF);
                let mem = match width {
                    1 => Mem::B1,
                    2 => Mem::B2,
                    4 => Mem::B4,
                    _ => Mem::B8,
                };
                return Ok(load_mem(v as usize + offset as usize, mem));
            }
        }
        self.call(cells[1].get() as FuncId, &[cells[0].get(), v])
    }

    /// `BoxesRunTime.equals(x, y)` when `x` and `y` are the same reference or boxes of the same
    /// `java.lang` class; `None` otherwise.
    pub fn boxed_equals(&mut self, x: u64, y: u64) -> Option<bool> {
        if x == y {
            return Some(true);
        }
        if x == 0 || y == 0 {
            return None;
        }
        let rtti = heap::read::<u64>(x as usize);
        if heap::read::<u64>(y as usize) != rtti {
            return None;
        }
        if self.funcs.boxes.is_none() {
            self.funcs.boxes = Some(self.value_boxes());
        }
        let &(_, offset, value) = self.funcs.boxes.as_ref()?.iter().find(|b| b.0 == rtti)?;
        let (x, y) = (x as usize + offset, y as usize + offset);
        Some(match value {
            BoxValue::Bits(mem) => load_mem(x, mem) == load_mem(y, mem),
            BoxValue::F32 => heap::read::<f32>(x) == heap::read::<f32>(y),
            BoxValue::F64 => heap::read::<f64>(x) == heap::read::<f64>(y),
        })
    }

    /// Type object, value field offset and value kind of each `java.lang` box class.
    fn value_boxes(&mut self) -> Vec<(u64, usize, BoxValue)> {
        let classes = ["Boolean", "Character", "Byte", "Short", "Integer", "Long", "Float", "Double"];
        let mut out = Vec::new();
        for short in classes {
            let name = self.world.top(&format!("java.lang.{short}"));
            let Ok(Some((rtti, field))) = self.value_field_of(name) else { continue };
            let value = match (short, field & 0xFF) {
                ("Float", _) => BoxValue::F32,
                ("Double", _) => BoxValue::F64,
                (_, 1) => BoxValue::Bits(Mem::B1),
                (_, 2) => BoxValue::Bits(Mem::B2),
                (_, 4) => BoxValue::Bits(Mem::B4),
                _ => BoxValue::Bits(Mem::B8),
            };
            out.push((rtti as u64, (field >> 8) as usize, value));
        }
        out
    }

    /// The type object of `java.lang` box class `ty` and its value field as `offset << 8 | width`.
    fn value_field(&mut self, ty: TypeId) -> Flow<Option<(usize, u64)>> {
        let Type::Ref { name, .. } = *self.world.pool.ty(ty) else { return Ok(None) };
        self.value_field_of(name)
    }

    fn value_field_of(&mut self, name: nir::GlobalId) -> Flow<Option<(usize, u64)>> {
        let class = self.world.pool.top_name(name).to_owned();
        if helper(&class, false).is_none() || !class.starts_with("java.lang.") {
            return Ok(None);
        }
        let field = self.world.member(name, &format!("F6_valuePT{}{class}", class.len()));
        let cls = self.link(name)?;
        let Some(f) = self.class(cls).fields.get(&field) else { return Ok(None) };
        let width = crate::layout::size_of(&self.world.pool, f.ty) as u64;
        Ok(Some((self.class(cls).rtti, (f.offset as u64) << 8 | width)))
    }

    fn box_helper(&mut self, ty: TypeId, boxing: bool) -> Flow<(u64, FuncId)> {
        let Type::Ref { name, .. } = *self.world.pool.ty(ty) else {
            return Err(VmError::Unsupported("box into a non-class type".into()).into());
        };
        let class = self.world.pool.top_name(name).to_owned();
        let (module, sig) =
            helper(&class, boxing).ok_or_else(|| VmError::Unsupported(format!("box {class}")))?;
        let owner = self.world.top(module);
        let module = self.module(owner)?;
        let g = self.world.member(owner, &sig);
        Ok((module as u64, self.func(g)?))
    }
}

/// The module and mangled method that box or unbox values of box class `class`.
fn helper(class: &str, boxing: bool) -> Option<(&'static str, String)> {
    let (module, prim, short, unboxed) = match class {
        "java.lang.Boolean" => ("scala.runtime.BoxesRunTime$", "z", "Boolean", "Boolean"),
        "java.lang.Character" => ("scala.runtime.BoxesRunTime$", "c", "Character", "Char"),
        "java.lang.Byte" => ("scala.runtime.BoxesRunTime$", "b", "Byte", "Byte"),
        "java.lang.Short" => ("scala.runtime.BoxesRunTime$", "s", "Short", "Short"),
        "java.lang.Integer" => ("scala.runtime.BoxesRunTime$", "i", "Integer", "Int"),
        "java.lang.Long" => ("scala.runtime.BoxesRunTime$", "j", "Long", "Long"),
        "java.lang.Float" => ("scala.runtime.BoxesRunTime$", "f", "Float", "Float"),
        "java.lang.Double" => ("scala.runtime.BoxesRunTime$", "d", "Double", "Double"),
        other => {
            let short = other.rsplit('.').next()?;
            let prim = match short {
                "UByte" => "b",
                "UShort" => "s",
                "UInt" => "i",
                "ULong" => "j",
                "USize" | "Size" => "w",
                s if s == "Ptr" || s == "CArray" || s == "CVarArgList" || s.starts_with("CFuncPtr") => "R_",
                _ => return None,
            };
            ("scala.scalanative.runtime.Boxes$", prim, short, short)
        }
    };
    let (name, tys) = match boxing {
        true => (format!("boxTo{short}"), format!("{prim}L{}{class}", class.len())),
        false => (format!("unboxTo{unboxed}"), format!("L16java.lang.Object{prim}")),
    };
    Some((module, format!("D{}{name}{tys}EO", name.len())))
}
