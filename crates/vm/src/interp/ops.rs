//! Operations the lowering leaves in NIR form, executed from the NIR instruction.

use super::Flow;
use crate::layout;
use crate::{Vm, VmError};
use nir::{Op, Val};

pub mod arith;
mod boxing;

impl Vm {
    pub fn op(&mut self, op: &Op, slots: &[u64]) -> Flow {
        match op {
            Op::Load { ty, ptr, .. } => {
                let addr = self.val(*ptr, slots)?;
                Ok(self.load_ty(addr as usize, *ty))
            }
            Op::Store { ty, ptr, value, .. } => {
                let (addr, v) = (self.val(*ptr, slots)?, self.val(*value, slots)?);
                self.store_ty(addr as usize, *ty, v);
                Ok(0)
            }
            Op::Elem { ty, ptr, indexes } => {
                let base = self.val(*ptr, slots)?;
                self.elem_addr(*ty, base, indexes, slots)
            }
            Op::Stackalloc { ty, n } => {
                let n = self.index(*n, slots)?.max(1) as usize;
                Ok(self.heap.alloc(n * layout::size_of(&self.world.pool, *ty) as usize) as u64)
            }
            Op::Fence(_) => Ok(0),
            Op::Classalloc { name, .. } => {
                let cls = self.link(*name)?;
                Ok(self.alloc_object(cls) as u64)
            }
            Op::Field { obj, name } => {
                let obj = self.val(*obj, slots)? as usize;
                if obj == 0 {
                    return self.null_pointer();
                }
                Ok((obj + self.field_offset(*name)?) as u64)
            }
            Op::As { ty, obj } => {
                let v = self.val(*obj, slots)?;
                if v != 0 && self.class_of_type(*ty)?.is_some() && !self.is_instance(v as usize, *ty)? {
                    let from = self.class(self.class_of(v as usize)).rtti as u64;
                    let to = self.class_of_type(*ty)?.map_or(0, |c| self.class(c).rtti as u64);
                    self.throw_runtime("D14throwClassCastR_R_nEO", &[from, to])?;
                }
                Ok(v)
            }
            Op::SizeOf(ty) => Ok(layout::size_of(&self.world.pool, *ty) as u64),
            Op::AlignmentOf(ty) => Ok(layout::align_of(&self.world.pool, *ty) as u64),
            Op::Var(_) => Ok(self.heap.alloc(8) as u64),
            Op::Varload(slot) => {
                let addr = self.val(*slot, slots)?;
                Ok(crate::heap::read(addr as usize))
            }
            Op::Varstore { slot, value } => {
                let (addr, v) = (self.val(*slot, slots)?, self.val(*value, slots)?);
                crate::heap::write(addr as usize, v);
                Ok(0)
            }
            Op::Arrayalloc { ty, init, .. } => self.array_alloc(*ty, *init, slots),
            Op::Extract { .. } => Err(VmError::Unsupported("op extract".into()).into()),
            Op::Insert { .. } => Err(VmError::Unsupported("op insert".into()).into()),
            Op::Dynmethod { .. } => Err(VmError::Unsupported("op dynmethod".into()).into()),
            _ => Err(VmError::Unsupported("op the lowering handles".into()).into()),
        }
    }

    fn array_alloc(&mut self, elem: nir::TypeId, init: nir::ValId, slots: &[u64]) -> Flow {
        let cls = self.array_class(elem)?;
        match self.world.pool.val(init).clone() {
            Val::ArrayValue(_, items) => {
                let arr = self.alloc_array(cls, items.len());
                for (i, item) in items.iter().enumerate() {
                    let v = self.val(*item, slots)?;
                    let addr = self.array_elem(arr as u64, i as u64, elem)?;
                    self.store_ty(addr, elem, v);
                }
                Ok(arr as u64)
            }
            _ => {
                let len = self.index(init, slots)?;
                if len < 0 {
                    return self.throw_new("java.lang.NegativeArraySizeException");
                }
                Ok(self.alloc_array(cls, len as usize) as u64)
            }
        }
    }
}
