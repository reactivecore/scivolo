//! Class metadata: class ids in hierarchy preorder with the range of their subclasses, each
//! class's `java.lang.Class` instance with its itable and vtable, and the class ids the
//! collector reads (`scalai_aot_gc_ids`, for `scalai_GC_set_ids`).

mod rtti;

use crate::R;
use crate::classes::{ClassId, Classes, Kind, array_names};
use crate::codegen::{CLASS, Decls};
use crate::reach::Reach;
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::Pool;

const RUNTIME: &str = "scala.scalanative.runtime.";

/// The primitive classes, in id order.
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

// Temporary until #113 aot 6 (library cache): the tables go into the program's object, not an
// object of their own.
pub fn define(module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls) -> R<()> {
    let mut fixed = Vec::new();
    for name in PRIMITIVES {
        fixed.push(reach.link_top(&format!("{RUNTIME}{name}"))?);
    }
    fixed.push(reach.link_top("java.lang.Object")?);
    fixed.push(reach.link_top(&format!("{RUNTIME}Array"))?);
    for name in array_names() {
        fixed.push(reach.link_top(&format!("{RUNTIME}{name}"))?);
    }
    let meta = reach.link_top(CLASS)?;
    let ids = ids(&reach.classes, &reach.world.pool);
    for (expected, &cls) in fixed.iter().enumerate() {
        if ids[cls as usize].0 != expected as i32 {
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
    gc_ids(module, reach, &ids)
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
    roots.sort_by_key(|&c| PRIMITIVES.iter().position(|p| *p == name(c)).unwrap_or(PRIMITIVES.len()));
    let rank = |c: ClassId| match name(c) {
        "Array" => 0,
        n => 1 + array_names().position(|a| a == n).unwrap_or(array_names().count()),
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
