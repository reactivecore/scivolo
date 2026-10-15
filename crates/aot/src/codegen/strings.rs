//! String constants: `java.lang.String` objects and their char arrays as data, outside the
//! collected heap.

use super::Decls;
use crate::R;
use crate::blob::Blob;
use crate::reach::Reach;
use cranelift_module::{DataId, Linkage, Module};
use cranelift_object::ObjectModule;
use vm::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE};

impl Decls {
    /// The `java.lang.String` holding `units`, defined on first use.
    pub fn string(&mut self, module: &mut ObjectModule, reach: &mut Reach, units: &[u16]) -> R<DataId> {
        if let Some(&id) = self.strings.get(units) {
            return Ok(id);
        }
        let n = self.strings.len();
        let string = reach.link_top("java.lang.String")?;
        let chars_cls = reach.link_top("scala.scalanative.runtime.CharArray")?;
        let mut chars = Blob::zeroed(ARRAY_HEADER as usize + 2 * units.len());
        chars.data.push((0, self.rtti(module, reach, chars_cls)?));
        chars.i32_at(ARRAY_LENGTH as i32, units.len() as i32);
        chars.i32_at(ARRAY_STRIDE as i32, 2);
        for (i, u) in units.iter().enumerate() {
            let at = ARRAY_HEADER as usize + 2 * i;
            chars.bytes[at..at + 2].copy_from_slice(&u.to_le_bytes());
        }
        let declare = |module: &mut ObjectModule, name: String| {
            module.declare_data(&name, Linkage::Local, true, false).map_err(|e| e.to_string())
        };
        let chars_id = declare(module, format!("scalai_aot_string.{n}.chars"))?;
        chars.define(module, chars_id)?;
        let field = |reach: &mut Reach, sig: &str| {
            let g = reach.world.member(reach.classes.get(string).name, sig);
            let f = reach.classes.get(string).fields.get(&g).ok_or_else(|| format!("no field String::{sig}"));
            f.map(|f| f.offset as i32)
        };
        let mut obj = Blob::zeroed(reach.classes.get(string).size as usize);
        obj.data.push((0, self.rtti(module, reach, string)?));
        obj.data.push((field(reach, "F5valueO")? as usize, chars_id));
        obj.i32_at(field(reach, "F5countO")?, units.len() as i32);
        let hash = units.iter().fold(0i32, |h, &u| h.wrapping_mul(31).wrapping_add(u as i32));
        obj.i32_at(field(reach, "F14cachedHashCodeO")?, hash);
        let id = declare(module, format!("scalai_aot_string.{n}"))?;
        obj.define(module, id)?;
        self.strings.insert(units.to_vec(), id);
        Ok(id)
    }
}
