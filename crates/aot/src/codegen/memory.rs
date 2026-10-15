//! Raw memory: loads and stores through pointers, element addresses, stack slots and `var`s.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, Value, types};
use cranelift_module::Module;
use nir::{Op, Type, TypeId, Val};
use vm::{align_of, size_of, struct_layout};

impl FnBuilder<'_, '_> {
    pub fn memory_op(&mut self, id: u64, op: &Op) -> R<Option<Value>> {
        let flags = MemFlagsData::new();
        Ok(Some(match op {
            Op::Load { ty, ptr, .. } => {
                let (t, p) = (self.ty(*ty)?, self.val(*ptr)?);
                self.b.ins().load(t, flags, p, 0)
            }
            Op::Store { ty, ptr, value, .. } => {
                let p = self.val(*ptr)?;
                let v = self.arg_of(*value, *ty)?;
                self.b.ins().store(flags, v, p, 0);
                return Ok(None);
            }
            Op::Elem { ty, ptr, indexes } => self.elem(*ty, *ptr, indexes)?,
            Op::Stackalloc { ty, n } => {
                let pool = &self.reach.world.pool;
                let (size, align) = (size_of(pool, *ty), align_of(pool, *ty));
                let count = match pool.val(*n) {
                    Val::Int(k) if *k >= 0 => *k as u32,
                    Val::Long(k) if *k >= 0 => *k as u32,
                    Val::Size(k) => *k as u32,
                    // Temporary until #115 (stackalloc shadow stack): dynamic counts take zeroed
                    // memory that is never freed and not scanned by the collector.
                    _ => {
                        let ptr = self.decls.ptr;
                        let n = self.val(*n)?;
                        let n = match self.b.func.dfg.value_type(n) == ptr {
                            true => n,
                            false => self.b.ins().sextend(ptr, n),
                        };
                        let bytes = self.b.ins().imul_imm_s(n, size as i64);
                        let alloc =
                            self.decls.import(self.module, "scalai_aot_stackalloc", &[ptr], Some(ptr))?;
                        let fref = self.module.declare_func_in_func(alloc, self.b.func);
                        let call = self.b.ins().call(fref, &[bytes]);
                        return Ok(Some(self.b.inst_results(call)[0]));
                    }
                };
                let bytes = size * count.max(1);
                let addr = self.stack_slot(bytes, align);
                let ptr = self.decls.ptr;
                let memset = self.decls.import(self.module, "memset", &[ptr, types::I32, ptr], Some(ptr))?;
                let (zero, len) =
                    (self.b.ins().iconst(types::I32, 0), self.b.ins().iconst(ptr, bytes as i64));
                let fref = self.module.declare_func_in_func(memset, self.b.func);
                self.b.ins().call(fref, &[addr, zero, len]);
                addr
            }
            Op::Var(ty) => {
                let pool = &self.reach.world.pool;
                let (size, align) = (size_of(pool, *ty), align_of(pool, *ty));
                self.var_types.insert(id, *ty);
                self.stack_slot(size, align)
            }
            Op::Varload(slot) => {
                let Val::Local(l, _) = *self.reach.world.pool.val(*slot) else {
                    return Err("varload".into());
                };
                let ty = *self.var_types.get(&l.0).ok_or("varload of an unknown var")?;
                let (t, p) = (self.ty(ty)?, self.val(*slot)?);
                self.b.ins().load(t, flags, p, 0)
            }
            Op::Varstore { slot, value } => {
                let Val::Local(l, _) = *self.reach.world.pool.val(*slot) else {
                    return Err("varstore".into());
                };
                let ty = *self.var_types.get(&l.0).ok_or("varstore of an unknown var")?;
                let p = self.val(*slot)?;
                let v = self.arg_of(*value, ty)?;
                self.b.ins().store(flags, v, p, 0);
                return Ok(None);
            }
            Op::SizeOf(ty) => {
                self.b.ins().iconst(self.decls.ptr, size_of(&self.reach.world.pool, *ty) as i64)
            }
            Op::AlignmentOf(ty) => {
                self.b.ins().iconst(self.decls.ptr, align_of(&self.reach.world.pool, *ty) as i64)
            }
            Op::Fence(_) => {
                self.b.ins().fence();
                return Ok(None);
            }
            op => return self.object_op(id, op),
        }))
    }

    fn stack_slot(&mut self, size: u32, align: u32) -> Value {
        let data = StackSlotData::new(StackSlotKind::ExplicitSlot, size.max(1), align.trailing_zeros() as u8);
        let slot = self.b.create_sized_stack_slot(data);
        self.b.ins().stack_addr(self.decls.ptr, slot, 0)
    }

    /// The address `elem ty, ptr, indexes`: the first index steps over `ty`s, the others select
    /// struct members or array elements.
    fn elem(&mut self, ty: TypeId, ptr: nir::ValId, indexes: &[nir::ValId]) -> R<Value> {
        let p = self.val(ptr)?;
        let size = size_of(&self.reach.world.pool, ty);
        let first = self.val(indexes[0])?;
        let first = match self.b.func.dfg.value_type(first) == self.decls.ptr {
            true => first,
            false => self.b.ins().sextend(self.decls.ptr, first),
        };
        let offset = self.b.ins().imul_imm_s(first, size as i64);
        let mut addr = self.b.ins().iadd(p, offset);
        let mut t = ty;
        for &ix in &indexes[1..] {
            let pool = &self.reach.world.pool;
            let Val::Int(i) = *pool.val(ix) else { return Err("a dynamic member index".into()) };
            let (at, member) = match pool.ty(t) {
                Type::StructValue(tys) => (struct_layout(pool, tys).1[i as usize], tys[i as usize]),
                Type::ArrayValue(elem, _) => (size_of(pool, *elem) * i as u32, *elem),
                t => return Err(format!("elem into {t:?}")),
            };
            addr = self.b.ins().iadd_imm_s(addr, at as i64);
            t = member;
        }
        Ok(addr)
    }
}
