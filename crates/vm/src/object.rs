//! Allocation of objects, arrays and strings, and reading strings back.

use crate::class::ClassId;
use crate::heap;
use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE};
use crate::{Vm, VmError};
use nir::StrId;

/// Where an allocation lives: in the collected heap, or forever, for what only the VM refers to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Space {
    Gc,
    Permanent,
}

impl Vm {
    pub fn alloc_object(&mut self, cls: ClassId) -> usize {
        self.alloc_object_in(cls, Space::Gc)
    }

    pub fn alloc_object_in(&mut self, cls: ClassId, space: Space) -> usize {
        let c = self.class(cls);
        let (size, rtti) = (c.size as usize, c.rtti);
        debug_assert!(rtti != 0, "instance of a class without metadata");
        match space {
            Space::Gc => self.heap.object(rtti, size),
            Space::Permanent => {
                let obj = self.heap.permanent(size);
                heap::write(obj, rtti);
                obj
            }
        }
    }

    pub fn alloc_array(&mut self, cls: ClassId, len: usize) -> usize {
        self.alloc_array_in(cls, len, Space::Gc)
    }

    pub fn alloc_array_in(&mut self, cls: ClassId, len: usize, space: Space) -> usize {
        let c = self.class(cls);
        let (elem, rtti) = (c.array_elem.unwrap_or(8) as usize, c.rtti);
        if space == Space::Gc {
            return self.heap.array(rtti, len, elem);
        }
        let arr = self.heap.permanent(ARRAY_HEADER as usize + elem * len);
        heap::write(arr, rtti);
        heap::write(arr + ARRAY_LENGTH, len as i32);
        heap::write(arr + ARRAY_STRIDE, elem as i32);
        arr
    }

    /// `size` zeroed bytes in the collected heap, scanned conservatively; they live while a
    /// frame or the native stack points into them.
    pub fn blob(&mut self, size: usize) -> Result<usize, VmError> {
        let cls = self.link_top("scala.scalanative.runtime.BlobArray")?;
        let arr = self.alloc_array(cls, size);
        heap::write(arr + ARRAY_STRIDE, -(size as i32));
        Ok(arr + ARRAY_HEADER as usize)
    }

    pub fn new_string(&mut self, s: &str) -> Result<usize, VmError> {
        self.new_string_in(s, Space::Gc)
    }

    pub fn new_string_in(&mut self, s: &str, space: Space) -> Result<usize, VmError> {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.new_string_utf16(&units, space)
    }

    pub fn new_string_utf16(&mut self, units: &[u16], space: Space) -> Result<usize, VmError> {
        let chars_cls = self.link_top("scala.scalanative.runtime.CharArray")?;
        let string_cls = self.link_top("java.lang.String")?;
        let chars = self.alloc_array_in(chars_cls, units.len(), space);
        for (i, &u) in units.iter().enumerate() {
            heap::write(chars + ARRAY_HEADER as usize + 2 * i, u);
        }
        let obj = self.alloc_object_in(string_cls, space);
        let hash = units.iter().fold(0i32, |h, &u| h.wrapping_mul(31).wrapping_add(u as i32));
        heap::write(obj + self.string_field("F5valueO")?, chars);
        heap::write(obj + self.string_field("F5countO")?, units.len() as i32);
        heap::write(obj + self.string_field("F14cachedHashCodeO")?, hash);
        Ok(obj)
    }

    fn string_field(&mut self, sig: &str) -> Result<usize, VmError> {
        let owner = self.world.top("java.lang.String");
        let g = self.world.member(owner, sig);
        self.field_offset(g)
    }

    pub fn string_literal(&mut self, s: StrId) -> Result<usize, VmError> {
        if let Some(&obj) = self.strings.get(&s) {
            return Ok(obj);
        }
        let units: Vec<u16> = match self.world.pool.utf16.get(&s) {
            Some(units) => units.to_vec(),
            None => self.world.pool.str(s).encode_utf16().collect(),
        };
        let obj = self.new_string_utf16(&units, Space::Permanent)?;
        self.strings.insert(s, obj);
        Ok(obj)
    }

    /// A C `argv` of `program` and `args`, NUL-terminated strings in the permanent heap.
    pub fn c_argv(&mut self, program: &str, args: &[String]) -> usize {
        let items: Vec<&str> = std::iter::once(program).chain(args.iter().map(String::as_str)).collect();
        let argv = self.heap.permanent(8 * (items.len() + 1));
        for (i, s) in items.iter().enumerate() {
            let c = self.heap.permanent(s.len() + 1);
            for (j, b) in s.bytes().enumerate() {
                heap::write(c + j, b);
            }
            heap::write(argv + 8 * i, c);
        }
        argv
    }

    /// The contents of the `java.lang.String` at `obj`.
    pub fn read_string(&mut self, obj: usize) -> Result<String, VmError> {
        if obj == 0 {
            return Ok("null".into());
        }
        let chars: usize = heap::read(obj + self.string_field("F5valueO")?);
        let offset: i32 = heap::read(obj + self.string_field("F6offsetO")?);
        let count: i32 = heap::read(obj + self.string_field("F5countO")?);
        let base = chars + ARRAY_HEADER as usize + 2 * offset as usize;
        let units: Vec<u16> = (0..count as usize).map(|i| heap::read(base + 2 * i)).collect();
        Ok(String::from_utf16_lossy(&units))
    }
}
