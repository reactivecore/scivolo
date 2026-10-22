//! Globals, types and targets in the snapshot file, by name, so that a process interns them
//! anew.

use super::Target;
use super::format::{In, Out};
use crate::World;
use nir::{GlobalId, Pool, Type, TypeId};

/// A global as its top-level name and member signature, empty for a top-level one.
pub fn put_global(out: &mut Out, pool: &Pool, g: GlobalId) {
    out.str(pool.top_name(g));
    out.str(pool.member_sig(g).unwrap_or(""));
}

pub fn get_global(input: &mut In, world: &mut World) -> Result<GlobalId, String> {
    let top = world.top(input.str()?);
    Ok(match input.str()? {
        "" => top,
        sig => world.member(top, sig),
    })
}

pub fn put_type(out: &mut Out, pool: &Pool, ty: TypeId) {
    let list = |out: &mut Out, tys: &[TypeId]| {
        out.u32(tys.len() as u32);
        tys.iter().for_each(|&t| put_type(out, pool, t));
    };
    match pool.ty(ty) {
        Type::ArrayValue(elem, n) => {
            out.u8(12);
            put_type(out, pool, *elem);
            out.u32(*n);
        }
        Type::StructValue(tys) => {
            out.u8(13);
            list(out, tys);
        }
        Type::Function(args, ret) => {
            out.u8(14);
            list(out, args);
            put_type(out, pool, *ret);
        }
        Type::Var(t) => {
            out.u8(18);
            put_type(out, pool, *t);
        }
        Type::Array { elem, nullable } => {
            out.u8(20);
            put_type(out, pool, *elem);
            out.u8(*nullable as u8);
        }
        Type::Ref { name, exact, nullable } => {
            out.u8(21);
            put_global(out, pool, *name);
            out.u8(*exact as u8 | (*nullable as u8) << 1);
        }
        simple => out.u8(SIMPLE.iter().position(|(_, t)| t == simple).expect("a simple type") as u8),
    }
}

/// The types without operands, by their tags.
const SIMPLE: [(u8, Type); 16] = [
    (0, Type::Vararg),
    (1, Type::Bool),
    (2, Type::Ptr),
    (3, Type::Char),
    (4, Type::Byte),
    (5, Type::Short),
    (6, Type::Int),
    (7, Type::Long),
    (8, Type::Float),
    (9, Type::Double),
    (10, Type::Size),
    (11, Type::Int128),
    (15, Type::Null),
    (16, Type::Nothing),
    (17, Type::Virtual),
    (19, Type::Unit),
];

pub fn get_type(input: &mut In, world: &mut World) -> Result<TypeId, String> {
    let tag = input.u8()?;
    let ty = match tag {
        12 => Type::ArrayValue(get_type(input, world)?, input.u32()?),
        13 => Type::StructValue(input.list(|i| get_type(i, world))?),
        14 => Type::Function(input.list(|i| get_type(i, world))?, get_type(input, world)?),
        18 => Type::Var(get_type(input, world)?),
        20 => Type::Array { elem: get_type(input, world)?, nullable: input.u8()? != 0 },
        21 => {
            let name = get_global(input, world)?;
            let flags = input.u8()?;
            Type::Ref { name, exact: flags & 1 != 0, nullable: flags & 2 != 0 }
        }
        _ => SIMPLE.iter().find(|(t, _)| *t == tag).ok_or("an unknown type tag")?.1.clone(),
    };
    Ok(TypeId(world.pool.types.intern(ty)))
}

pub fn put_target(out: &mut Out, target: &Target) {
    let (tag, a, b) = match target {
        Target::Trampoline(top, sig) => (0, top, sig.as_str()),
        Target::Function(top, sig) => (1, top, sig.as_str()),
        Target::Symbol(name) => (2, name, ""),
    };
    out.u8(tag);
    out.str(a);
    out.str(b);
}

pub fn get_target(input: &mut In) -> Result<Target, String> {
    let tag = input.u8()?;
    let (a, b) = (input.str()?.to_owned(), input.str()?.to_owned());
    match tag {
        0 => Ok(Target::Trampoline(a, b)),
        1 => Ok(Target::Function(a, b)),
        2 => Ok(Target::Symbol(a)),
        _ => Err("an unknown target tag".into()),
    }
}
