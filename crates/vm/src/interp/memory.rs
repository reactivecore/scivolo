use super::Flow;
use crate::heap;
use crate::layout::{self, ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, VmError};
use nir::{Type, TypeId, ValId};

impl Vm {
    /// Loads a value of type `ty`, zero-extended to 64 bits.
    pub fn load_ty(&self, addr: usize, ty: TypeId) -> u64 {
        // SAFETY: interpreted code only loads from memory it allocated or received from C.
        unsafe {
            match layout::size_of(&self.world.pool, ty) {
                1 => heap::load::<u8>(addr) as u64,
                2 => heap::load::<u16>(addr) as u64,
                4 => heap::load::<u32>(addr) as u64,
                _ => heap::load::<u64>(addr),
            }
        }
    }

    pub fn store_ty(&self, addr: usize, ty: TypeId, v: u64) {
        // SAFETY: interpreted code only stores to memory it allocated or received from C.
        unsafe {
            match layout::size_of(&self.world.pool, ty) {
                1 => heap::store(addr, v as u8),
                2 => heap::store(addr, v as u16),
                4 => heap::store(addr, v as u32),
                _ => heap::store(addr, v),
            }
        }
    }

    /// The signed integer value of index operand `v`.
    pub fn index(&mut self, v: ValId, slots: &[u64]) -> Flow<i64> {
        let raw = self.val(v, slots)?;
        Ok(match self.val_ty(v) {
            Type::Int => raw as i32 as i64,
            Type::Short => raw as i16 as i64,
            Type::Byte => raw as i8 as i64,
            Type::Char => raw as u16 as i64,
            _ => raw as i64,
        })
    }

    /// The address `elem[ty] base, indexes...` designates, as LLVM's `getelementptr`.
    pub fn elem_addr(&mut self, ty: TypeId, base: u64, indexes: &[ValId], slots: &[u64]) -> Flow {
        let Some((&first, rest)) = indexes.split_first() else { return Ok(base) };
        let mut addr = base
            .wrapping_add_signed(self.index(first, slots)? * layout::size_of(&self.world.pool, ty) as i64);
        let mut ty = ty;
        for &i in rest {
            let i = self.index(i, slots)?;
            match self.world.pool.ty(ty).clone() {
                Type::StructValue(tys) => {
                    let (_, offsets) = layout::struct_layout(&self.world.pool, &tys);
                    addr += offsets[i as usize] as u64;
                    ty = tys[i as usize];
                }
                Type::ArrayValue(elem, _) => {
                    addr = addr.wrapping_add_signed(i * layout::size_of(&self.world.pool, elem) as i64);
                    ty = elem;
                }
                other => return Err(VmError::Unsupported(format!("elem into {other:?}")).into()),
            }
        }
        Ok(addr)
    }

    /// The address of element `idx` of array `arr`, after null and bounds checks.
    pub fn array_elem(&mut self, arr: u64, idx: u64, elem: TypeId) -> Flow<usize> {
        if arr == 0 {
            return self.null_pointer();
        }
        let arr = arr as usize;
        // SAFETY: `arr` is a live array, whose header holds its length.
        let len: i32 = unsafe { heap::load(arr + ARRAY_LENGTH) };
        let i = idx as u32 as i32;
        if i < 0 || i >= len {
            self.throw_runtime("D16throwOutOfBoundsiinEO", &[i as u32 as u64, len as u32 as u64])?;
        }
        Ok(arr + ARRAY_HEADER as usize + i as usize * layout::size_of(&self.world.pool, elem) as usize)
    }

    pub fn array_length(&mut self, arr: u64) -> Flow {
        if arr == 0 {
            return self.null_pointer();
        }
        // SAFETY: `arr` is a live array, whose header holds its length.
        Ok(unsafe { heap::load::<u32>(arr as usize + ARRAY_LENGTH) } as u64)
    }
}
