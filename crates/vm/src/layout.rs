//! Sizes and alignments of NIR types, following Scala Native's `MemoryLayout` on 64-bit targets.

use nir::{Pool, Type, TypeId};

pub const WORD: u32 = 8;
pub const OBJECT_HEADER: u32 = WORD;
pub const ARRAY_HEADER: u32 = 16;
pub const ARRAY_LENGTH: usize = 8;
pub const ARRAY_STRIDE: usize = 12;

pub fn size_of(pool: &Pool, ty: TypeId) -> u32 {
    match pool.ty(ty) {
        Type::Bool | Type::Byte => 1,
        Type::Char | Type::Short => 2,
        Type::Int | Type::Float => 4,
        Type::Int128 => 16,
        Type::ArrayValue(elem, n) => size_of(pool, *elem) * n,
        Type::StructValue(tys) => struct_layout(pool, tys).0,
        _ => WORD,
    }
}

pub fn align_of(pool: &Pool, ty: TypeId) -> u32 {
    match pool.ty(ty) {
        Type::Bool | Type::Byte => 1,
        Type::Char | Type::Short => 2,
        Type::Int | Type::Float => 4,
        Type::Int128 => 16,
        Type::ArrayValue(elem, _) => align_of(pool, *elem),
        Type::StructValue(tys) => tys.iter().map(|t| align_of(pool, *t)).max().unwrap_or(1),
        _ => WORD,
    }
}

/// The total size of a struct of `tys` and the offset of each member.
pub fn struct_layout(pool: &Pool, tys: &[TypeId]) -> (u32, Vec<u32>) {
    let (mut offset, mut max_align) = (0u32, 1u32);
    let offsets = tys
        .iter()
        .map(|&t| {
            let align = align_of(pool, t);
            offset = offset.next_multiple_of(align);
            let at = offset;
            offset += size_of(pool, t);
            max_align = max_align.max(align);
            at
        })
        .collect();
    (offset.next_multiple_of(max_align), offsets)
}
