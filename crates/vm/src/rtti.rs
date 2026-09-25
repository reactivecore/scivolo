//! Runtime type information: every class's `java.lang.Class` object, laid out as Scala Native expects.

use crate::class::{self, ClassId, Kind};
use crate::heap;
use crate::{Vm, VmError};

const PRIMITIVES: [&str; 11] = [
    "PrimitiveByte",
    "PrimitiveShort",
    "PrimitiveInt",
    "PrimitiveLong",
    "PrimitiveChar",
    "PrimitiveFloat",
    "PrimitiveDouble",
    "PrimitiveBoolean",
    "PrimitiveUnit",
    "RawSize",
    "RawPtr",
];

#[derive(Default)]
pub struct Rtti {
    meta: Option<ClassId>,
    meta_size: usize,
    pending: Vec<ClassId>,
    unnamed: Vec<ClassId>,
    strings_ready: bool,
    next_class: i32,
    next_trait: i32,
}

impl Vm {
    pub fn bootstrap(&mut self) -> Result<(), VmError> {
        self.rtti.next_class = 23;
        let meta = self.link_top("java.lang.Class")?;
        self.rtti.meta = Some(meta);
        self.rtti.meta_size = self.class(meta).size as usize;
        for id in std::mem::take(&mut self.rtti.pending) {
            self.init_rtti(id)?;
        }
        self.link_top("java.lang.String")?;
        self.link_top("scala.scalanative.runtime.CharArray")?;
        self.rtti.strings_ready = true;
        for id in std::mem::take(&mut self.rtti.unnamed) {
            self.name_rtti(id)?;
        }
        Ok(())
    }

    pub fn init_rtti(&mut self, id: ClassId) -> Result<(), VmError> {
        let Some(meta) = self.rtti.meta.or_else(|| (self.class_name(id) == "java.lang.Class").then_some(id))
        else {
            self.rtti.pending.push(id);
            return Ok(());
        };
        let meta_size = self.class(meta).size as usize;
        let addr = self.heap.alloc(meta_size + 8);
        self.classes.all[id as usize].rtti = addr;
        let meta_rtti = self.class(meta).rtti;
        let class_id = self.class_id_value(id);
        let c = self.class(id);
        let (size, parent_rtti) = (c.size, c.parent.map_or(0, |p| self.class(p).rtti));
        let set = |vm: &mut Vm, field: &str, value: u64, wide: bool| -> Result<(), VmError> {
            let g = vm.world.top("java.lang.Class");
            let g = vm.world.member(g, field);
            let at = addr + vm.field_offset(g)?;
            // SAFETY: `at` lies inside the freshly allocated metadata object.
            unsafe {
                match wide {
                    true => heap::store(at, value),
                    false => heap::store(at, value as u32),
                }
            }
            Ok(())
        };
        // SAFETY: `addr` is a fresh allocation of `meta_size + 8` bytes.
        unsafe {
            heap::store(addr, meta_rtti);
            heap::store(addr + meta_size, id);
        }
        set(self, "F2idO", class_id as u32 as u64, false)?;
        set(self, "F4sizeO", size as u64, false)?;
        set(self, "F12idRangeUntilO", class_id as u32 as u64, false)?;
        set(self, "F10superClassO", parent_rtti as u64, true)?;
        match self.rtti.strings_ready {
            true => self.name_rtti(id),
            false => {
                self.rtti.unnamed.push(id);
                Ok(())
            }
        }
    }

    fn name_rtti(&mut self, id: ClassId) -> Result<(), VmError> {
        let name = self.class_name(id).to_owned();
        let s = self.new_string(&name)?;
        let g = self.world.top("java.lang.Class");
        let g = self.world.member(g, "F4nameO");
        let at = self.class(id).rtti + self.field_offset(g)?;
        // SAFETY: `at` lies inside the metadata object of class `id`.
        unsafe { heap::store(at, s) };
        Ok(())
    }

    fn class_id_value(&mut self, id: ClassId) -> i32 {
        let name = self.class_name(id).to_owned();
        let short = name.strip_prefix("scala.scalanative.runtime.").unwrap_or("");
        if let Some(i) = PRIMITIVES.iter().position(|p| *p == short) {
            return i as i32;
        }
        if let Some(i) = class::array_names().position(|a| a == short) {
            return 13 + i as i32;
        }
        match (name.as_str(), self.class(id).kind) {
            ("java.lang.Object", _) => 11,
            ("scala.scalanative.runtime.Array", _) => 12,
            (_, Kind::Trait) => {
                self.rtti.next_trait -= 1;
                self.rtti.next_trait
            }
            _ => {
                self.rtti.next_class += 1;
                self.rtti.next_class
            }
        }
    }

    pub fn class_name(&self, id: ClassId) -> &str {
        self.world.pool.top_name(self.class(id).name)
    }

    /// The class of the object at `obj`, read from its header.
    pub fn class_of(&self, obj: usize) -> ClassId {
        // SAFETY: `obj` is a live object, whose header points to a metadata object of
        // `meta_size + 8` bytes ending in the class id.
        unsafe {
            let rtti: usize = heap::load(obj);
            heap::load(rtti + self.rtti.meta_size)
        }
    }
}
