//! Data objects: bytes with relocations to data and functions at their offsets.

use crate::R;
use cranelift_module::{DataDescription, DataId, FuncId, Module};
use cranelift_object::ObjectModule;

#[derive(Default)]
pub struct Blob {
    pub bytes: Vec<u8>,
    pub data: Vec<(usize, DataId)>,
    pub funcs: Vec<(usize, FuncId)>,
}

impl Blob {
    pub fn zeroed(size: usize) -> Self {
        Blob { bytes: vec![0; size], ..Blob::default() }
    }

    pub fn i32_at(&mut self, at: i32, v: i32) {
        self.bytes[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Defines data object `id` with these contents, aligned to 8 bytes.
    pub fn define(self, module: &mut ObjectModule, id: DataId) -> R<()> {
        let mut desc = DataDescription::new();
        desc.set_align(8);
        desc.define(self.bytes.into_boxed_slice());
        for (at, d) in self.data {
            let gv = module.declare_data_in_data(d, &mut desc);
            desc.write_data_addr(at as u32, gv, 0);
        }
        for (at, f) in self.funcs {
            let fref = module.declare_func_in_data(f, &mut desc);
            desc.write_function_addr(at as u32, fref);
        }
        module.define_data(id, &desc).map_err(|e| e.to_string())
    }
}
