//! Instance layout: inherited fields keep their parent's offsets, own fields follow.

use super::{ClassId, Field, InstanceLayout};
use crate::layout::{self, ARRAY_HEADER, OBJECT_HEADER};
use crate::{Vm, VmError};
use nir::{Attr, DefnKind, GlobalId};
use rustc_hash::FxHashMap;

impl Vm {
    pub(super) fn field_layout(
        &mut self,
        name: GlobalId,
        parent: Option<ClassId>,
        is_array: bool,
    ) -> Result<InstanceLayout, VmError> {
        let mut fields = FxHashMap::default();
        let mut order = Vec::new();
        let mut start = if is_array { ARRAY_HEADER } else { OBJECT_HEADER };
        if let Some(p) = parent {
            let parent = self.class(p);
            for f in &parent.field_order {
                let field = &parent.fields[f];
                fields.insert(*f, Field { offset: field.offset, ty: field.ty });
                order.push(*f);
                start = start.max(field.offset + layout::size_of(&self.world.pool, field.ty));
            }
        }
        let mut own = Vec::new();
        let mut specs = Vec::new();
        for m in self.world.members(name)?.to_vec() {
            let defn = self.world.defn(m)?;
            let DefnKind::Var { ty, .. } = defn.kind else { continue };
            let align = defn.attrs.iter().find_map(|a| match a {
                Attr::Align { size, group } => Some((*size, *group)),
                _ => None,
            });
            own.push(m);
            specs.push(layout::Field { ty, align });
        }
        let (size, offsets) = layout::aligned_fields(&self.world.pool, start, &specs);
        for ((f, spec), offset) in own.iter().zip(&specs).zip(offsets) {
            fields.insert(*f, Field { offset, ty: spec.ty });
            order.push(*f);
        }
        Ok(InstanceLayout { fields, order, size })
    }
}
