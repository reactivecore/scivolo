//! The classpath's resource files embedded in the object (`scivolo_aot_resources`, read by
//! `c/resources.c`), as the VM serves them (ADR 0007).

use crate::R;
use crate::blob::Blob;
use cranelift_module::{Linkage, Module};
use cranelift_object::ObjectModule;
use vm::World;

/// The size of an entry: path, its length, content, its length.
const ENTRY: usize = 32;

pub fn define(module: &mut ObjectModule, world: &mut World) -> R<()> {
    let all = vm::embedded_resources(world);
    let mut table = Blob::zeroed(ENTRY * all.len().max(1));
    for (i, (root, path)) in all.iter().enumerate() {
        let content = vm::embedded_content(world, *root, path);
        let at = ENTRY * i;
        let path = path.clone().into_bytes();
        table.bytes[at + 8..at + 16].copy_from_slice(&(path.len() as u64).to_le_bytes());
        table.bytes[at + 24..at + 32].copy_from_slice(&(content.len() as u64).to_le_bytes());
        for (offset, mut bytes, suffix) in [(at, path, "path"), (at + 16, content, "content")] {
            bytes.push(0);
            let name = format!("scivolo_aot_resource.{i}.{suffix}");
            let id = module.declare_data(&name, Linkage::Local, false, false).map_err(|e| e.to_string())?;
            Blob { bytes, ..Blob::default() }.define(module, id)?;
            table.data.push((offset, id));
        }
    }
    let id = module
        .declare_data("scivolo_aot_resources", Linkage::Export, false, false)
        .map_err(|e| e.to_string())?;
    table.define(module, id)?;
    let mut count = Blob::zeroed(8);
    count.bytes.copy_from_slice(&(all.len() as u64).to_le_bytes());
    let id = module
        .declare_data("scivolo_aot_resources_count", Linkage::Export, false, false)
        .map_err(|e| e.to_string())?;
    count.define(module, id)
}
