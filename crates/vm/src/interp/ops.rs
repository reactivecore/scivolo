use super::Flow;
use crate::layout;
use crate::{Vm, VmError};
use nir::{Op, Type, Val};

mod arith;

impl Vm {
    pub fn op(&mut self, op: &Op, slots: &[u64]) -> Flow {
        match op {
            Op::Call { ptr, args, .. } => {
                let target = self.val(*ptr, slots)?;
                let args = args.iter().map(|&a| self.val(a, slots)).collect::<Flow<Vec<_>>>()?;
                self.call_value(target, &args)
            }
            Op::Bin { bin, ty, l, r } => {
                let (l, r) = (self.val(*l, slots)?, self.val(*r, slots)?);
                let ty = self.world.pool.ty(*ty).clone();
                self.bin(*bin, &ty, l, r)
            }
            Op::Comp { comp, ty, l, r } => {
                let (l, r) = (self.val(*l, slots)?, self.val(*r, slots)?);
                Ok(arith::comp(*comp, self.world.pool.ty(*ty), l, r) as u64)
            }
            Op::Conv { conv, ty, value } => {
                let from = self.val_ty(*value);
                let v = self.val(*value, slots)?;
                Ok(arith::conv(*conv, &from, self.world.pool.ty(*ty), v))
            }
            Op::Load { ty, ptr, .. } => {
                let addr = self.val(*ptr, slots)?;
                Ok(self.load_ty(addr as usize, *ty))
            }
            Op::Store { ty, ptr, value, .. } => {
                let (addr, v) = (self.val(*ptr, slots)?, self.val(*value, slots)?);
                self.store_ty(addr as usize, *ty, v);
                self.unit_value()
            }
            Op::Elem { ty, ptr, indexes } => {
                let base = self.val(*ptr, slots)?;
                self.elem_addr(*ty, base, indexes, slots)
            }
            Op::Stackalloc { ty, n } => {
                let n = self.index(*n, slots)?.max(1) as usize;
                Ok(self.heap.alloc(n * layout::size_of(&self.world.pool, *ty) as usize) as u64)
            }
            Op::Fence(_) => self.unit_value(),
            Op::Classalloc { name, .. } => {
                let cls = self.link(*name)?;
                Ok(self.alloc_object(cls) as u64)
            }
            Op::Fieldload { ty, obj, name } => {
                let addr = self.field_addr(*obj, *name, slots)?;
                Ok(self.load_ty(addr, *ty))
            }
            Op::Fieldstore { ty, obj, name, value } => {
                let addr = self.field_addr(*obj, *name, slots)?;
                let v = self.val(*value, slots)?;
                self.store_ty(addr, *ty, v);
                self.unit_value()
            }
            Op::Field { obj, name } => Ok(self.field_addr(*obj, *name, slots)? as u64),
            Op::Method { obj, sig } => {
                let obj = self.val(*obj, slots)? as usize;
                if obj == 0 {
                    return self.null_pointer();
                }
                let f = self.virtual_method(obj, *sig)?;
                self.callable(f)
            }
            Op::Module(name) => Ok(self.module(*name)? as u64),
            Op::Is { ty, obj } => {
                let obj = self.val(*obj, slots)? as usize;
                Ok(self.is_instance(obj, *ty)? as u64)
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
            Op::Copy(v) => self.val(*v, slots),
            Op::SizeOf(ty) => Ok(layout::size_of(&self.world.pool, *ty) as u64),
            Op::AlignmentOf(ty) => Ok(layout::align_of(&self.world.pool, *ty) as u64),
            Op::Box { ty, obj } => self.boxing(*ty, *obj, slots, true),
            Op::Unbox { ty, obj } => self.boxing(*ty, *obj, slots, false),
            Op::Var(_) => Ok(self.heap.alloc(8) as u64),
            Op::Varload(slot) => {
                let addr = self.val(*slot, slots)?;
                // SAFETY: `addr` is a cell allocated by `Op::Var`.
                Ok(unsafe { crate::heap::load(addr as usize) })
            }
            Op::Varstore { slot, value } => {
                let (addr, v) = (self.val(*slot, slots)?, self.val(*value, slots)?);
                // SAFETY: `addr` is a cell allocated by `Op::Var`.
                unsafe { crate::heap::store(addr as usize, v) };
                self.unit_value()
            }
            Op::Arrayalloc { ty, init, .. } => self.array_alloc(*ty, *init, slots),
            Op::Arrayload { ty, arr, idx } => {
                let (arr, idx) = (self.val(*arr, slots)?, self.val(*idx, slots)?);
                let addr = self.array_elem(arr, idx, *ty)?;
                Ok(self.load_ty(addr, *ty))
            }
            Op::Arraystore { ty, arr, idx, value } => {
                let (arr, idx, v) =
                    (self.val(*arr, slots)?, self.val(*idx, slots)?, self.val(*value, slots)?);
                let addr = self.array_elem(arr, idx, *ty)?;
                self.store_ty(addr, *ty, v);
                self.unit_value()
            }
            Op::Arraylength(arr) => {
                let arr = self.val(*arr, slots)?;
                self.array_length(arr)
            }
            other => Err(VmError::Unsupported(format!("op {}", op_name(other))).into()),
        }
    }

    /// The result of an operation that yields no value; nothing reads it.
    fn unit_value(&mut self) -> Flow {
        Ok(0)
    }

    fn field_addr(&mut self, obj: nir::ValId, name: nir::GlobalId, slots: &[u64]) -> Flow<usize> {
        let obj = self.val(obj, slots)? as usize;
        if obj == 0 {
            return self.null_pointer();
        }
        Ok(obj + self.field_offset(name)?)
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
                    let exc = self.world.top("java.lang.NegativeArraySizeException");
                    return Err(VmError::Unsupported(format!("negative array size ({exc:?})")).into());
                }
                Ok(self.alloc_array(cls, len as usize) as u64)
            }
        }
    }

    fn boxing(&mut self, ty: nir::TypeId, obj: nir::ValId, slots: &[u64], boxing: bool) -> Flow {
        let Type::Ref { name, .. } = *self.world.pool.ty(ty) else {
            return Err(VmError::Unsupported("box into a non-class type".into()).into());
        };
        let class = self.world.pool.top_name(name).to_owned();
        let v = self.val(obj, slots)?;
        let helper =
            arith::box_helper(&class, boxing).ok_or_else(|| VmError::Unsupported(format!("box {class}")))?;
        let owner = self.world.top(helper.0);
        let module = self.module(owner)?;
        let g = self.world.member(owner, &helper.1);
        self.call_global(g, &[module as u64, v])
    }
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Extract { .. } => "extract",
        Op::Insert { .. } => "insert",
        Op::Dynmethod { .. } => "dynmethod",
        _ => "?",
    }
}
