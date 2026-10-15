//! Instance fields and vtable slots of a class: inherited ones keep their places, own ones follow.

use super::{ClassId, Classes, Field, is_virtual};
use crate::R;
use nir::{Attr, DefnKind, GlobalId, StrId};
use rustc_hash::FxHashMap;
use vm::World;
use vm::layout::{self, ARRAY_HEADER, OBJECT_HEADER};

const ARRAYS: [&str; 10] = [
    "BooleanArray",
    "CharArray",
    "ByteArray",
    "ShortArray",
    "IntArray",
    "LongArray",
    "FloatArray",
    "DoubleArray",
    "ObjectArray",
    "BlobArray",
];

/// Whether `name` is one of the runtime's array classes.
pub fn is_array(name: &str) -> bool {
    name.strip_prefix("scala.scalanative.runtime.").is_some_and(|short| ARRAYS.contains(&short))
}

pub fn array_names() -> impl Iterator<Item = &'static str> {
    ARRAYS.iter().copied()
}

impl Classes {
    pub fn fields(
        &self,
        world: &mut World,
        name: GlobalId,
        parent: Option<ClassId>,
    ) -> R<(FxHashMap<GlobalId, Field>, u32)> {
        let err = |e: vm::LoadError| e.to_string();
        let mut fields = FxHashMap::default();
        let mut start = if is_array(world.pool.top_name(name)) { ARRAY_HEADER } else { OBJECT_HEADER };
        if let Some(p) = parent {
            for (&f, field) in &self.get(p).fields {
                fields.insert(f, Field { offset: field.offset, ty: field.ty });
                start = start.max(field.offset + layout::size_of(&world.pool, field.ty));
            }
        }
        let mut own = Vec::new();
        let mut specs = Vec::new();
        for m in world.fields(name).map_err(err)? {
            let Some((ty, attrs)) = world.var(m).map_err(err)? else { continue };
            let align = attrs.iter().find_map(|a| match a {
                Attr::Align { size, group } => Some((*size, *group)),
                _ => None,
            });
            own.push(m);
            specs.push(layout::Field { ty, align });
        }
        let (size, offsets) = layout::aligned_fields(&world.pool, start, &specs);
        for ((f, spec), offset) in own.into_iter().zip(&specs).zip(offsets) {
            fields.insert(f, Field { offset, ty: spec.ty });
        }
        Ok((fields, size))
    }

    pub fn slots(
        &self,
        world: &mut World,
        name: GlobalId,
        parent: Option<ClassId>,
    ) -> R<(Vec<StrId>, FxHashMap<StrId, u32>)> {
        let (mut slots, mut slot_of) = match parent {
            Some(p) => (self.get(p).slots.clone(), self.get(p).slot_of.clone()),
            None => Default::default(),
        };
        let members = world.members(name).map_err(|e| e.to_string())?.to_vec();
        for m in members {
            let &nir::Global::Member(_, sig) = world.pool.global(m) else { continue };
            if !is_virtual(world.pool.str(sig)) {
                continue;
            }
            let defn = world.defn(m).map_err(|e| e.to_string())?;
            if !matches!(defn.kind, DefnKind::Define { .. } | DefnKind::Declare { .. }) {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(e) = slot_of.entry(sig) {
                e.insert(slots.len() as u32);
                slots.push(sig);
            }
        }
        Ok((slots, slot_of))
    }
}
