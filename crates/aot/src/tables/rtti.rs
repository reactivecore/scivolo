//! One class's metadata: a `java.lang.Class` instance laid out as Scala Native's runtime reads
//! it; for a class, followed by its itable (pointer, length) and vtable.

use crate::R;
use crate::blob::Blob;
use crate::classes::{ClassId, Kind, method_key};
use crate::codegen::Decls;
use crate::reach::Reach;
use cranelift_module::{DataId, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::{StrId, Type};

/// A local data object named after `cls` and `suffix`, holding `blob`.
fn side_table(module: &mut ObjectModule, reach: &Reach, cls: ClassId, suffix: &str, blob: Blob) -> R<DataId> {
    let name = format!("_S{}.{suffix}", reach.world.pool.mangle(reach.classes.get(cls).name));
    let id = module.declare_data(&name, Linkage::Local, false, false).map_err(|e| e.to_string())?;
    blob.define(module, id)?;
    Ok(id)
}

/// The interfaces `Class.getInterfaces` reports, as the VM links them: the direct traits not
/// implied by the superclass or another direct trait; for an array class, `runtime.Array`'s
/// traits in reverse.
fn direct_traits(reach: &Reach, cls: ClassId) -> Vec<ClassId> {
    let classes = &reach.classes;
    let c = classes.get(cls);
    let array = c
        .parent
        .filter(|&p| reach.world.pool.top_name(classes.get(p).name) == "scala.scalanative.runtime.Array");
    if let Some(p) = array {
        return classes.get(p).traits.iter().rev().copied().collect();
    }
    let implied = |t: ClassId| {
        c.parent.is_some_and(|p| classes.get(p).ancestors.contains(&t))
            || c.traits.iter().any(|&u| u != t && classes.get(u).ancestors.contains(&t))
    };
    c.traits.iter().copied().filter(|&t| !implied(t)).collect()
}

/// A table of the metadata addresses of `classes`.
fn rtti_list(module: &mut ObjectModule, reach: &Reach, decls: &mut Decls, classes: &[ClassId]) -> R<Blob> {
    let mut list = Blob::zeroed(8 * classes.len().max(1));
    for (i, &t) in classes.iter().enumerate() {
        list.data.push((8 * i, decls.rtti(module, reach, t)?));
    }
    Ok(list)
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
    let (id, interfaces, count, name_at) = (l.id, l.interfaces, l.interfaces_count, l.name as usize);
    let c = reach.classes.get(cls);
    let is_trait = c.kind == Kind::Trait;
    let size = if is_trait { l.itable } else { l.vtable + 8 * c.slots.len() as i32 };
    let mut blob = Blob { bytes: vec![0; size as usize], ..Blob::default() };
    blob.data.push((0, decls.rtti(module, reach, meta)?));
    blob.i32_at(id, ids[cls as usize].0);
    let name: Vec<u16> = vm::java_name(reach.world.pool.top_name(c.name)).encode_utf16().collect();
    blob.data.push((name_at, decls.string(module, reach, &name)?));
    let c = reach.classes.get(cls);
    let mut all: Vec<ClassId> = c
        .ancestors
        .iter()
        .copied()
        .filter(|&a| a != cls && reach.classes.get(a).kind == Kind::Trait)
        .collect();
    all.sort_by_key(|&t| ids[t as usize].0);
    let direct = direct_traits(reach, cls);
    blob.i32_at(count, direct.len() as i32);
    let list = rtti_list(module, reach, decls, &direct)?;
    blob.data.push((interfaces as usize, side_table(module, reach, cls, "interfaces", list)?));
    if !is_trait {
        let (at, len_at) = (decls.rtti.traits as usize, decls.rtti.traits_len as usize);
        let list = rtti_list(module, reach, decls, &all)?;
        blob.data.push((at, side_table(module, reach, cls, "traits", list)?));
        blob.bytes[len_at..len_at + 8].copy_from_slice(&(all.len() as u64).to_le_bytes());
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
    blob.i32_at(l.itables_count, -1);
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
        let called = reach.vcalls.contains(&sig);
        match target(module, reach, decls, cls, sig, true)? {
            Some(f) => blob.funcs.push((vtable as usize + 8 * i, f)),
            None if called => {
                let (class, method) =
                    (reach.world.name(reach.classes.get(cls).name), reach.world.pool.str(sig));
                return Err(format!("{class} has no implementation of {method}"));
            }
            None => {}
        }
    }
    let mut entries = Vec::new();
    for sig in reach.icalls.iter().copied().collect::<Vec<StrId>>() {
        if let Some(f) = target(module, reach, decls, cls, sig, false)? {
            entries.push((method_key(reach.world.pool.str(sig)), sig, f));
        }
    }
    entries.sort_by_key(|e| e.0);
    if let Some(w) = entries.windows(2).find(|w| w[0].0 == w[1].0) {
        let pool = &reach.world.pool;
        return Err(format!("itable keys of {} and {} collide", pool.str(w[0].1), pool.str(w[1].1)));
    }
    let mut table = Blob { bytes: vec![0; 16 * entries.len().max(1)], ..Blob::default() };
    for (i, (key, _, f)) in entries.iter().enumerate() {
        table.bytes[16 * i..16 * i + 8].copy_from_slice(&key.to_le_bytes());
        table.funcs.push((16 * i + 8, *f));
    }
    let at = itable_len as usize;
    blob.bytes[at..at + 8].copy_from_slice(&(entries.len() as u64).to_le_bytes());
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
