//! Instance fields and vtable slots of a class: inherited ones keep their places, own ones follow.

use super::{ClassId, Classes, is_virtual};
use crate::R;
use nir::{DefnKind, GlobalId, StrId};
use rustc_hash::FxHashMap;
use vm::{FieldSlot, World};

/// Whether `name` is one of the runtime's array classes.
pub fn is_array(name: &str) -> bool {
    name.strip_prefix("scala.scalanative.runtime.").is_some_and(|short| vm::array_names().any(|a| a == short))
}

impl Classes {
    /// The fields of class `name`, inherited ones first, as the VM lays them out, and the size.
    pub fn fields(
        &self,
        world: &mut World,
        name: GlobalId,
        parent: Option<ClassId>,
    ) -> R<(Vec<FieldSlot>, u32)> {
        let inherited = parent.map_or(&[][..], |p| &self.get(p).field_list[..]);
        let array = is_array(world.pool.top_name(name));
        vm::instance_fields(world, name, inherited, array).map_err(|e| e.to_string())
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
