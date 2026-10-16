//! Instance layout: inherited fields keep their parent's offsets, own fields follow.

use super::{ClassId, Field, InstanceLayout};
use crate::layout::{self, ARRAY_HEADER, OBJECT_HEADER};
use crate::world::LoadError;
use crate::{Vm, VmError, World};
use nir::{Attr, GlobalId, TypeId};
use rustc_hash::FxHashMap;

/// A field, its offset and its type.
pub type FieldSlot = (GlobalId, u32, TypeId);

impl Vm {
    pub(super) fn field_layout(
        &mut self,
        name: GlobalId,
        parent: Option<ClassId>,
        is_array: bool,
    ) -> Result<InstanceLayout, VmError> {
        let inherited: Vec<FieldSlot> = match parent {
            Some(p) => {
                let parent = self.class(p);
                parent
                    .field_order
                    .iter()
                    .map(|f| (*f, parent.fields[f].offset, parent.fields[f].ty))
                    .collect()
            }
            None => Vec::new(),
        };
        let (all, size) = instance_fields(&mut self.world, name, &inherited, is_array)?;
        let order = all.iter().map(|f| f.0).collect();
        let fields =
            all.into_iter().map(|(f, offset, ty)| (f, Field { offset, ty })).collect::<FxHashMap<_, _>>();
        Ok(InstanceLayout { fields, order, size })
    }
}

/// The fields of instances of class `name` whose parent has the fields `inherited`, in order,
/// with their offsets as Scala Native places them, and the instance size.
pub fn instance_fields(
    world: &mut World,
    name: GlobalId,
    inherited: &[FieldSlot],
    is_array: bool,
) -> Result<(Vec<FieldSlot>, u32), LoadError> {
    let mut all = inherited.to_vec();
    let mut start = if is_array { ARRAY_HEADER } else { OBJECT_HEADER };
    for &(_, offset, ty) in inherited {
        start = start.max(offset + layout::size_of(&world.pool, ty));
    }
    let mut own = Vec::new();
    let mut specs = Vec::new();
    for m in world.fields(name)? {
        let Some((ty, attrs)) = world.var(m)? else { continue };
        let align = attrs.iter().find_map(|a| match a {
            Attr::Align { size, group } => Some((*size, *group)),
            _ => None,
        });
        own.push(m);
        specs.push(layout::Field { ty, align });
    }
    let (size, offsets) = layout::aligned_fields(&world.pool, start, &specs);
    all.extend(own.into_iter().zip(&specs).zip(offsets).map(|((f, spec), offset)| (f, offset, spec.ty)));
    Ok((all, size))
}
