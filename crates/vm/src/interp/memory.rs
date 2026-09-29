use super::Flow;
use super::code::Mem;
use crate::heap;
use crate::layout::{self, ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, VmError};
use nir::{Type, TypeId, ValId};

/// The address of element `idx` of size `size` of array `arr`, if `arr` is not null and `idx`
/// in bounds.
#[inline(always)]
pub fn array_elem(arr: u64, idx: u64, size: usize) -> Option<usize> {
    if arr == 0 {
        return None;
    }
    let len: i32 = heap::read(arr as usize + ARRAY_LENGTH);
    let i = idx as u32 as i32;
    (i >= 0 && i < len).then(|| arr as usize + ARRAY_HEADER as usize + i as usize * size)
}

/// Loads a value accessed as `mem`, zero-extended to 64 bits; an aggregate's value is its
/// address.
#[inline]
pub fn load_mem(addr: usize, mem: Mem) -> u64 {
    match mem {
        Mem::B1 => heap::read::<u8>(addr) as u64,
        Mem::B2 => heap::read::<u16>(addr) as u64,
        Mem::B4 => heap::read::<u32>(addr) as u64,
        Mem::B8 => heap::read::<u64>(addr),
        Mem::Aggregate(_) => addr as u64,
    }
}

/// Stores `v` accessed as `mem`; an aggregate is copied from the address `v`, or zeroed if `v`
/// is 0.
#[inline]
pub fn store_mem(addr: usize, mem: Mem, v: u64) {
    match mem {
        Mem::B1 => heap::write(addr, v as u8),
        Mem::B2 => heap::write(addr, v as u16),
        Mem::B4 => heap::write(addr, v as u32),
        Mem::B8 => heap::write(addr, v),
        Mem::Aggregate(size) if v != 0 => heap::copy(v as usize, addr, size as usize),
        Mem::Aggregate(size) => heap::zero(addr, size as usize),
    }
}

impl Vm {
    /// Loads a value of type `ty`, zero-extended to 64 bits.
    pub fn load_ty(&self, addr: usize, ty: TypeId) -> u64 {
        load_mem(addr, Mem::of(&self.world.pool, ty))
    }

    pub fn store_ty(&self, addr: usize, ty: TypeId, v: u64) {
        store_mem(addr, Mem::of(&self.world.pool, ty), v)
    }

    pub fn is_aggregate(&self, ty: TypeId) -> bool {
        matches!(self.world.pool.ty(ty), Type::StructValue(_) | Type::ArrayValue(..))
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
        let size = layout::size_of(&self.world.pool, elem) as usize;
        self.array_elem_sized(arr, idx, size)
    }

    /// The address of element `idx` of `size` bytes in array `arr`, checking bounds.
    #[inline]
    pub fn array_elem_sized(&mut self, arr: u64, idx: u64, size: usize) -> Flow<usize> {
        if let Some(at) = array_elem(arr, idx, size) {
            return Ok(at);
        }
        if arr == 0 {
            return self.null_pointer();
        }
        let len: i32 = heap::read(arr as usize + ARRAY_LENGTH);
        self.throw_runtime("D16throwOutOfBoundsiinEO", &[idx as u32 as u64, len as u32 as u64])?;
        unreachable!("throws")
    }

    pub fn array_length(&mut self, arr: u64) -> Flow {
        if arr == 0 {
            return self.null_pointer();
        }
        Ok(heap::read::<u32>(arr as usize + ARRAY_LENGTH) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::{Mem, store_mem};

    #[test]
    fn aggregate_store_copies_or_zeroes() {
        let (src, mut dst) = ([7u8; 8], [0xffu8; 8]);
        store_mem(dst.as_mut_ptr() as usize, Mem::Aggregate(8), src.as_ptr() as u64);
        assert_eq!(dst, [7; 8]);
        store_mem(dst.as_mut_ptr() as usize, Mem::Aggregate(8), 0);
        assert_eq!(dst, [0; 8]);
    }
}
