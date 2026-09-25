//! Allocation of objects, arrays and strings, and reading strings back.

use crate::class::ClassId;
use crate::heap;
use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE};
use crate::{Vm, VmError};
use nir::StrId;

impl Vm {
    pub fn alloc_object(&mut self, cls: ClassId) -> usize {
        let c = self.class(cls);
        let (size, rtti) = (c.size as usize, c.rtti);
        let obj = self.heap.alloc(size);
        heap::write(obj, rtti);
        obj
    }

    pub fn alloc_array(&mut self, cls: ClassId, len: usize) -> usize {
        let c = self.class(cls);
        let (elem, rtti) = (c.array_elem.unwrap_or(8) as usize, c.rtti);
        let arr = self.heap.alloc(ARRAY_HEADER as usize + elem * len);
        heap::write(arr, rtti);
        heap::write(arr + ARRAY_LENGTH, len as i32);
        heap::write(arr + ARRAY_STRIDE, elem as i32);
        arr
    }

    pub fn new_string(&mut self, s: &str) -> Result<usize, VmError> {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.new_string_utf16(&units)
    }

    pub fn new_string_utf16(&mut self, units: &[u16]) -> Result<usize, VmError> {
        let chars_cls = self.link_top("scala.scalanative.runtime.CharArray")?;
        let chars = self.alloc_array(chars_cls, units.len());
        for (i, &u) in units.iter().enumerate() {
            heap::write(chars + ARRAY_HEADER as usize + 2 * i, u);
        }
        let string_cls = self.link_top("java.lang.String")?;
        let obj = self.alloc_object(string_cls);
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
        let text = self.world.pool.str(s).to_owned();
        let obj = self.new_string(&text)?;
        self.strings.insert(s, obj);
        Ok(obj)
    }

    pub fn string_array(&mut self, items: &[String]) -> Result<usize, VmError> {
        let cls = self.link_top("scala.scalanative.runtime.ObjectArray")?;
        let arr = self.alloc_array(cls, items.len());
        for (i, s) in items.iter().enumerate() {
            let obj = self.new_string(s)?;
            heap::write(arr + ARRAY_HEADER as usize + 8 * i, obj);
        }
        Ok(arr)
    }

    /// `toString()` of throwable `exc`, or its class name if that fails too.
    pub fn describe_throwable(&mut self, exc: usize) -> String {
        let name = self.class_name(self.class_of(exc)).to_owned();
        let sig = nir::StrId(self.world.pool.strings.intern("D8toStringL16java.lang.StringEO".into()));
        let text = self
            .virtual_method(exc, sig)
            .and_then(|f| self.call(f, &[exc as u64]))
            .ok()
            .and_then(|s| self.read_string(s as usize).ok());
        text.unwrap_or(name)
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
