//! Instance layout: the offset of every field, inherited ones first.

use super::{ClassId, Field, InstanceLayout};
use crate::layout::{self, ARRAY_HEADER, OBJECT_HEADER};
use crate::{Vm, VmError};
use nir::{Attr, DefnKind, GlobalId, Type, TypeId};
use rustc_hash::FxHashMap;

impl Vm {
    pub(super) fn field_layout(
        &mut self,
        name: GlobalId,
        parent: Option<ClassId>,
        is_array: bool,
    ) -> Result<InstanceLayout, VmError> {
        let mut order: Vec<GlobalId> = parent.map(|p| self.class(p).field_order.clone()).unwrap_or_default();
        for m in self.world.members(name)?.to_vec() {
            if let DefnKind::Var { .. } = self.world.defn(m)?.kind {
                order.push(m);
            }
        }
        let mut specs = Vec::with_capacity(order.len());
        for &f in &order {
            let defn = self.world.defn(f)?;
            let DefnKind::Var { ty, .. } = defn.kind else { unreachable!("fields are vars") };
            let align = defn.attrs.iter().find_map(|a| match a {
                Attr::Align { size, group } => Some((*size, *group)),
                _ => None,
            });
            specs.push(layout::Field { ty, align });
        }
        let header = if is_array { ARRAY_HEADER } else { OBJECT_HEADER };
        let ptr = self.world.pool.types.intern(Type::Ptr);
        let header_ty = TypeId(self.world.pool.types.intern(Type::ArrayValue(TypeId(ptr), header / 8)));
        let (size, offsets) = match specs.iter().any(|s| s.align.is_some()) {
            true => layout::aligned_fields(&self.world.pool, header_ty, &specs),
            false => {
                let all: Vec<TypeId> = std::iter::once(header_ty).chain(specs.iter().map(|s| s.ty)).collect();
                let (size, offsets) = layout::struct_layout(&self.world.pool, &all);
                (size, offsets[1..].to_vec())
            }
        };
        let fields: FxHashMap<GlobalId, Field> = order
            .iter()
            .zip(specs.iter().zip(offsets))
            .map(|(&f, (spec, offset))| (f, Field { offset, ty: spec.ty }))
            .collect();
        Ok(InstanceLayout { fields, order, size })
    }
}
