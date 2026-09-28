//! `box` and `unbox`, which call Scala Native's box helpers; unboxing a `java.lang` box reads
//! its value field directly.

use crate::interp::code::Mem;
use crate::interp::memory::load_mem;
use crate::interp::{Flow, FuncId};
use crate::{Vm, VmError, heap};
use nir::{Type, TypeId};
use std::cell::Cell;

/// The mangled static forwarder `BoxesRunTime.equals(Object, Object): Boolean`.
const BOXED_EQUALS: &str = "D6equalsL16java.lang.ObjectL16java.lang.ObjectzEo";

/// The box classes whose instances `boxed_equals` compares by value.
const VALUE_BOXES: [&str; 8] = [
    "java.lang.Boolean",
    "java.lang.Character",
    "java.lang.Byte",
    "java.lang.Short",
    "java.lang.Integer",
    "java.lang.Long",
    "java.lang.Float",
    "java.lang.Double",
];

/// Whether `g` is the static `BoxesRunTime.equals(Object, Object)` that generic `==` calls.
pub fn is_boxed_equals(pool: &nir::Pool, g: nir::GlobalId) -> bool {
    pool.top_name(g) == "scala.runtime.BoxesRunTime" && pool.member_sig(g) == Some(BOXED_EQUALS)
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

    /// `BoxesRunTime.equals(x, y)` when `x` and `y` are the same reference, `x` is null, or both
    /// are boxes of the same `java.lang` class; `None` otherwise.
    pub fn boxed_equals(&mut self, x: u64, y: u64) -> Option<bool> {
        if x == y {
            return Some(true);
        }
        if x == 0 {
            return Some(false);
        }
        if y == 0 {
            return None;
        }
        let rtti = heap::read::<u64>(x as usize);
        if heap::read::<u64>(y as usize) != rtti {
            return None;
        }
        let (offset, mem, float) = self.box_value(rtti)?;
        let (a, b) = (load_mem(x as usize + offset, mem), load_mem(y as usize + offset, mem));
        Some(match (float, mem) {
            (true, Mem::B4) => f32::from_bits(a as u32) == f32::from_bits(b as u32),
            (true, _) => f64::from_bits(a) == f64::from_bits(b),
            (false, _) => a == b,
        })
    }

    /// The value field of the box class with type object `rtti` as offset, access and whether
    /// it holds a floating-point number, if it is one of `VALUE_BOXES`; cached per type object.
    fn box_value(&mut self, rtti: u64) -> Option<(usize, Mem, bool)> {
        if let Some(&known) = self.funcs.boxes.get(&rtti) {
            return known;
        }
        let cls = self.class_of_rtti(rtti as usize);
        let name = self.class(cls).name;
        let class = self.world.pool.top_name(name).to_owned();
        let value = match VALUE_BOXES.contains(&class.as_str()) {
            false => None,
            true => {
                let field = self.world.member(name, &format!("F6_valuePT{}{class}", class.len()));
                let float = class.ends_with("Float") || class.ends_with("Double");
                let f = self.class(cls).fields.get(&field);
                f.map(|f| (f.offset as usize, Mem::of(&self.world.pool, f.ty), float))
            }
        };
        self.funcs.boxes.insert(rtti, value);
        value
    }

    /// The type object of `java.lang` box class `ty` and its value field as `offset << 8 | width`.
    fn value_field(&mut self, ty: TypeId) -> Flow<Option<(usize, u64)>> {
        let Type::Ref { name, .. } = *self.world.pool.ty(ty) else { return Ok(None) };
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
