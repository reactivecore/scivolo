//! Class metadata: class ids in hierarchy preorder with the range of their subclasses, each
//! class's `java.lang.Class` instance with its itable and vtable, the class ids the collector
//! reads (`scalai_aot_gc_ids`, for `scalai_GC_set_ids`), and the array of all classes.

mod rtti;

use crate::R;
use crate::classes::{ClassId, Classes, Kind};
use crate::codegen::{CLASS, Decls};
use crate::reach::Reach;
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::Pool;

const RUNTIME: &str = "scala.scalanative.runtime.";

// Temporary until #113 aot 6 (library cache): the tables go into the program's object, not an
// object of their own.
pub fn define(module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls) -> R<()> {
    let fixed = link_runtime(reach)?;
    reach.link_top("java.lang.String")?;
    let meta = reach.link_top(CLASS)?;
    let ids = ids(&reach.classes, &reach.world.pool);
    for &(expected, cls) in &fixed {
        if ids[cls as usize].0 != expected {
            let name = reach.world.name(reach.classes.get(cls).name);
            return Err(format!(
                "{name} has class id {}, the runtime expects {expected}",
                ids[cls as usize].0
            ));
        }
    }
    for cls in 0..reach.classes.all.len() as ClassId {
        rtti::define(module, reach, decls, &ids, meta, cls)?;
    }
    if !reach.queue.is_empty() {
        return Err("a function was reached after compilation".into());
    }
    gc_ids(module, reach, &ids)?;
    all_classes(module, reach, decls)?;
    layout(module, reach, decls)
}

/// `scalai_aot_classes`: an `Array[Class]` of every class, which `Class.forName` searches.
fn all_classes(module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls) -> R<()> {
    let n = reach.classes.all.len();
    let array = reach.link_top(&format!("{RUNTIME}ObjectArray"))?;
    let header = vm::ARRAY_HEADER as usize;
    let mut blob = crate::blob::Blob::zeroed(header + 8 * n);
    blob.data.push((0, decls.rtti(module, reach, array)?));
    blob.i32_at(vm::ARRAY_LENGTH as i32, n as i32);
    blob.i32_at(vm::ARRAY_STRIDE as i32, 8);
    for cls in 0..n {
        blob.data.push((header + 8 * cls, decls.rtti(module, reach, cls as ClassId)?));
    }
    blob.define(module, decls.all_classes)
}

/// `scalai_aot_rtti_offsets` (a class's id, last subclass id, traits, number of traits) for
/// `Class.is`, and `scalai_aot_layout` (the class name, `String`'s `value` and `count`, the array
/// header size) for the C stub to print a class name.
fn layout(module: &mut ObjectModule, reach: &mut Reach, decls: &Decls) -> R<()> {
    let l = &decls.rtti;
    define_i32s(module, "scalai_aot_rtti_offsets", &[l.id, l.range, l.traits, l.traits_len])?;
    let string = reach.link_top("java.lang.String")?;
    let mut values = vec![l.name];
    for sig in ["F5valueO", "F5countO"] {
        let g = reach.world.member(reach.classes.get(string).name, sig);
        let f = reach.classes.get(string).fields.get(&g).ok_or_else(|| format!("no field String::{sig}"))?;
        values.push(f.offset as i32);
    }
    values.push(vm::ARRAY_HEADER as i32);
    define_i32s(module, "scalai_aot_layout", &values)
}

fn define_i32s(module: &mut ObjectModule, name: &str, values: &[i32]) -> R<()> {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let data = module.declare_data(name, Linkage::Export, false, false).map_err(|e| e.to_string())?;
    let mut desc = DataDescription::new();
    desc.define(bytes.into_boxed_slice());
    module.define_data(data, &desc).map_err(|e| e.to_string())
}

/// Links the classes with ids fixed by the runtime, the primitives, `Object` and the arrays:
/// each with the id the VM gives it.
pub fn link_runtime(reach: &mut Reach) -> R<Vec<(i32, ClassId)>> {
    let mut fixed = Vec::new();
    for (i, (name, _)) in vm::PRIMITIVES.iter().enumerate() {
        fixed.push((i as i32, reach.link_top(&format!("{RUNTIME}{name}"))?));
    }
    fixed.push((vm::FIRST_ARRAY - 2, reach.link_top("java.lang.Object")?));
    fixed.push((vm::FIRST_ARRAY - 1, reach.link_top(&format!("{RUNTIME}Array"))?));
    for (i, name) in vm::array_names().enumerate() {
        fixed.push((vm::FIRST_ARRAY + i as i32, reach.link_top(&format!("{RUNTIME}{name}"))?));
    }
    Ok(fixed)
}

/// Each class's id and the last id of its subclasses; traits count down from -1.
fn ids(classes: &Classes, pool: &Pool) -> Vec<(i32, i32)> {
    let name = |c: ClassId| pool.top_name(classes.get(c).name).strip_prefix(RUNTIME).unwrap_or("");
    let mut children = vec![Vec::new(); classes.all.len()];
    let mut roots = Vec::new();
    for (i, c) in classes.all.iter().enumerate().filter(|(_, c)| c.kind != Kind::Trait) {
        match c.parent {
            Some(p) => children[p as usize].push(i as ClassId),
            None => roots.push(i as ClassId),
        }
    }
    let primitives = &vm::PRIMITIVES;
    roots.sort_by_key(|&c| primitives.iter().position(|p| p.0 == name(c)).unwrap_or(primitives.len()));
    let rank = |c: ClassId| match name(c) {
        "Array" => 0,
        n => 1 + vm::array_names().position(|a| a == n).unwrap_or(vm::array_names().count()),
    };
    for list in &mut children {
        list.sort_by_key(|&c| rank(c));
    }
    let mut out = vec![(0, 0); classes.all.len()];
    let mut next = 0;
    let mut stack: Vec<(ClassId, bool)> = roots.iter().rev().map(|&r| (r, false)).collect();
    while let Some((c, done)) = stack.pop() {
        if done {
            out[c as usize].1 = next - 1;
            continue;
        }
        out[c as usize].0 = next;
        next += 1;
        stack.push((c, true));
        stack.extend(children[c as usize].iter().rev().map(|&k| (k, false)));
    }
    let traits = classes.all.iter().enumerate().filter(|(_, c)| c.kind == Kind::Trait);
    for (n, (i, _)) in traits.enumerate() {
        out[i] = (-1 - n as i32, -1 - n as i32);
    }
    out
}

fn gc_ids(module: &mut ObjectModule, reach: &mut Reach, ids: &[(i32, i32)]) -> R<()> {
    let mut find = |name: &str| {
        let g = reach.world.top(name);
        reach.classes.find(g)
    };
    let id = |c: Option<ClassId>| c.map_or(-1, |c| ids[c as usize].0);
    let array = find(&format!("{RUNTIME}Array")).ok_or("no runtime.Array")?;
    let object_array = id(find(&format!("{RUNTIME}ObjectArray")));
    let blob_array = id(find(&format!("{RUNTIME}BlobArray")));
    let boxed_ptr = id(find("scala.scalanative.unsafe.Ptr"));
    let weak = match find("java.lang.ref.WeakReference") {
        Some(w) => {
            let referent = reach.world.member(reach.classes.get(w).name, "F21_gc_modified_referentO");
            let offset = reach.classes.get(w).fields.get(&referent).map_or(-1, |f| f.offset as i32);
            [ids[w as usize].0, ids[w as usize].1, offset]
        }
        None => [1, 0, -1],
    };
    let (lo, hi) = ids[array as usize];
    let values = [object_array, blob_array, lo, hi, boxed_ptr, weak[0], weak[1], weak[2]];
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let data =
        module.declare_data("scalai_aot_gc_ids", Linkage::Export, false, false).map_err(|e| e.to_string())?;
    let mut desc = DataDescription::new();
    desc.set_align(8);
    desc.define(bytes.into_boxed_slice());
    module.define_data(data, &desc).map_err(|e| e.to_string())
}
