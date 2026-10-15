//! Where the parts of a class's metadata are: a `java.lang.Class` instance, then its itable and
//! vtable.

use crate::R;
use crate::classes::Classes;
use crate::reach::Reach;

pub struct RttiLayout {
    pub id: i32,
    pub range: i32,
    pub size: i32,
    pub name: i32,
    pub interfaces: i32,
    pub interfaces_count: i32,
    pub ref_offsets: i32,
    pub super_class: i32,
    /// Scala Native's itable size; negative makes `Class.is` search the interfaces.
    pub itables_count: i32,
    pub itable: i32,
    pub itable_len: i32,
    pub vtable: i32,
}

pub const CLASS: &str = "java.lang.Class";

impl RttiLayout {
    pub fn new(reach: &mut Reach) -> R<Self> {
        let meta = reach.link_top(CLASS)?;
        let classes: &Classes = &reach.classes;
        let mut field = |sig: &str| {
            let g = reach.world.member(classes.get(meta).name, sig);
            let f = classes.get(meta).fields.get(&g).ok_or_else(|| format!("no field {CLASS}::{sig}"))?;
            Ok::<_, String>(f.offset as i32)
        };
        for (sig, offset) in vm::GC_RTTI_FIELDS {
            if field(sig)? != offset as i32 {
                return Err(format!("{CLASS}::{sig} is not where Immix reads it"));
            }
        }
        let header = classes.get(meta).size.next_multiple_of(8) as i32;
        Ok(RttiLayout {
            id: field("F2idO")?,
            range: field("F12idRangeUntilO")?,
            size: field("F4sizeO")?,
            name: field("F4nameO")?,
            interfaces: field("F10interfacesO")?,
            interfaces_count: field("F15interfacesCountO")?,
            ref_offsets: field("F15refFieldOffsetsO")?,
            super_class: field("F10superClassO")?,
            itables_count: field("F12itablesCountO")?,
            itable: header,
            itable_len: header + 8,
            vtable: header + 16,
        })
    }
}
