// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   tools/src/main/scala/scala/scalanative/codegen/MemoryLayout.scala
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

/// Padding width of `@align` fields whose width is resolved at link time (`contendedPaddingWidth`).
const CONTENDED_PADDING: u32 = 64;

/// An instance field and its `@align(size, group)`, if any.
pub struct Field {
    pub ty: TypeId,
    pub align: Option<(i32, Option<nir::StrId>)>,
}

/// The size of an object whose `fields`, some aligned, follow at `start`, and each field's
/// offset in `fields` order; as Scala Native's `MemoryLayout.ofAlignedFields`, fields are placed
/// ordered by alignment group, and each aligned group starts and ends on its alignment.
pub fn aligned_fields(pool: &Pool, start: u32, fields: &[Field]) -> (u32, Vec<u32>) {
    let width = |size: i32| if size < 0 { CONTENDED_PADDING } else { size as u32 };
    let mut order: Vec<usize> = (0..fields.len()).collect();
    order.sort_by_key(|&i| fields[i].align.and_then(|(_, g)| g).map(|g| pool.str(g)));
    let (mut offset, mut max_align) = (start, WORD);
    let mut offsets = vec![0; fields.len()];
    let mut place = |i: usize, align: u32, offset: &mut u32| {
        max_align = max_align.max(align);
        *offset = offset.next_multiple_of(align);
        offsets[i] = *offset;
        *offset += size_of(pool, fields[i].ty);
    };
    let mut at = 0;
    while at < order.len() {
        let head = &fields[order[at]];
        let group = head.align.and_then(|(_, g)| g);
        let len = match group {
            Some(_) => {
                order[at..].iter().take_while(|&&i| fields[i].align.and_then(|(_, g)| g) == group).count()
            }
            None => 1,
        };
        let members = &order[at..at + len];
        let align = members.iter().filter_map(|&i| fields[i].align.map(|(s, _)| width(s))).max();
        if let Some(a) = align {
            offset = offset.next_multiple_of(a);
        }
        for (k, &i) in members.iter().enumerate() {
            let natural = align_of(pool, fields[i].ty);
            place(i, if k == 0 { align.unwrap_or(natural) } else { natural }, &mut offset);
        }
        if let Some(a) = align {
            offset = offset.next_multiple_of(a);
        }
        at += len;
    }
    (offset.next_multiple_of(max_align), offsets)
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
