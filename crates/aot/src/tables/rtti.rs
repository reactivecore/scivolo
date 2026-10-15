//! One class's metadata: a `java.lang.Class` instance laid out as Scala Native's runtime reads
//! it; for a class, followed by its itable (pointer, length) and vtable.

use crate::R;
use crate::classes::{ClassId, Kind, method_key};
use crate::codegen::Decls;
use crate::reach::Reach;
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::{StrId, Type};

/// Bytes with relocations to data and functions at their offsets.
#[derive(Default)]
struct Blob {
    bytes: Vec<u8>,
    data: Vec<(usize, DataId)>,
    funcs: Vec<(usize, FuncId)>,
}

impl Blob {
    fn i32_at(&mut self, at: i32, v: i32) {
        self.bytes[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn define(self, module: &mut ObjectModule, id: DataId) -> R<()> {
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

/// A local data object named after `cls` and `suffix`, holding `blob`.
fn side_table(module: &mut ObjectModule, reach: &Reach, cls: ClassId, suffix: &str, blob: Blob) -> R<DataId> {
    let name = format!("_S{}.{suffix}", reach.world.pool.mangle(reach.classes.get(cls).name));
    let id = module.declare_data(&name, Linkage::Local, false, false).map_err(|e| e.to_string())?;
    blob.define(module, id)?;
    Ok(id)
}

pub fn define(
    module: &mut ObjectModule,
    reach: &mut Reach,
    decls: &mut Decls,
    ids: &[(i32, i32)],
    meta: ClassId,
    cls: ClassId,
) -> R<()> {
    let l = &decls.rtti;
    let (id, interfaces, count) = (l.id, l.interfaces, l.interfaces_count);
    let c = reach.classes.get(cls);
    let is_trait = c.kind == Kind::Trait;
    let size = if is_trait { l.itable } else { l.vtable + 8 * c.slots.len() as i32 };
    let mut blob = Blob { bytes: vec![0; size as usize], ..Blob::default() };
    blob.data.push((0, decls.rtti(module, reach, meta)?));
    // Temporary until #113 aot 3 (strings): the class name stays null.
    blob.i32_at(id, ids[cls as usize].0);
    let mut traits: Vec<ClassId> = c
        .ancestors
        .iter()
        .copied()
        .filter(|&a| a != cls && reach.classes.get(a).kind == Kind::Trait)
        .collect();
    traits.sort_by_key(|&t| ids[t as usize].0);
    blob.i32_at(count, traits.len() as i32);
    let mut list = Blob { bytes: vec![0; 8 * traits.len().max(1)], ..Blob::default() };
    for (i, &t) in traits.iter().enumerate() {
        list.data.push((8 * i, decls.rtti(module, reach, t)?));
    }
    blob.data.push((interfaces as usize, side_table(module, reach, cls, "interfaces", list)?));
    if !is_trait {
        class_part(module, reach, decls, ids, cls, &mut blob)?;
    }
    let data = decls.rtti(module, reach, cls)?;
    blob.define(module, data)
}

/// Size, id range, reference fields, superclass, itable and vtable of class `cls`.
fn class_part(
    module: &mut ObjectModule,
    reach: &mut Reach,
    decls: &mut Decls,
    ids: &[(i32, i32)],
    cls: ClassId,
    blob: &mut Blob,
) -> R<()> {
    let l = &decls.rtti;
    let (size, range, refs, sup, itable, itable_len, vtable) =
        (l.size, l.range, l.ref_offsets, l.super_class, l.itable, l.itable_len, l.vtable);
    let c = reach.classes.get(cls);
    blob.i32_at(size, c.size as i32);
    blob.i32_at(range, ids[cls as usize].1);
    let pool = &reach.world.pool;
    let mut offsets: Vec<i32> = c
        .fields
        .values()
        .filter(|f| matches!(pool.ty(f.ty), Type::Ref { .. } | Type::Array { .. } | Type::Null | Type::Unit))
        .map(|f| f.offset as i32)
        .collect();
    offsets.sort_unstable();
    offsets.push(-1);
    let (parent, slots) = (c.parent, c.slots.clone());
    let offsets = Blob { bytes: offsets.iter().flat_map(|o| o.to_le_bytes()).collect(), ..Blob::default() };
    blob.data.push((refs as usize, side_table(module, reach, cls, "refs", offsets)?));
    if let Some(p) = parent {
        blob.data.push((sup as usize, decls.rtti(module, reach, p)?));
    }
    if !reach.is_allocated(cls) {
        return Ok(());
    }
    for (i, sig) in slots.into_iter().enumerate() {
        if let Some(f) = target(module, reach, decls, cls, sig, true)? {
            blob.funcs.push((vtable as usize + 8 * i, f));
        }
    }
    let mut entries = Vec::new();
    for sig in reach.icalls.iter().copied().collect::<Vec<StrId>>() {
        if let Some(f) = target(module, reach, decls, cls, sig, false)? {
            entries.push((method_key(reach.world.pool.str(sig)), f));
        }
    }
    entries.sort_by_key(|e| e.0);
    let mut table = Blob { bytes: vec![0; 16 * entries.len().max(1)], ..Blob::default() };
    for (i, (key, f)) in entries.iter().enumerate() {
        table.bytes[16 * i..16 * i + 8].copy_from_slice(&key.to_le_bytes());
        table.funcs.push((16 * i + 8, *f));
    }
    blob.i32_at(itable_len, entries.len() as i32);
    blob.data.push((itable as usize, side_table(module, reach, cls, "itable", table)?));
    Ok(())
}

/// The implementation of `sig` in `cls`, if it is called through a vtable (`virtual`) or an
/// itable and `cls` has one.
fn target(
    module: &mut ObjectModule,
    reach: &mut Reach,
    decls: &mut Decls,
    cls: ClassId,
    sig: StrId,
    virtual_: bool,
) -> R<Option<FuncId>> {
    let called = if virtual_ { reach.vcalls.contains(&sig) } else { reach.icalls.contains(&sig) };
    if !called {
        return Ok(None);
    }
    match reach.classes.resolve(reach.world, cls, sig) {
        Some(g) => decls.func(module, reach, g).map(Some),
        None => Ok(None),
    }
}
