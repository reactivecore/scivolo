//! The bytes of a class file.

use crate::layout::{Laid, LaidMember};
use crate::{Code, MAJOR_VERSION, Pool};

/// A class whose entries are all in its pool.
pub struct Class {
    pub access: u16,
    pub this: u16,
    pub superclass: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<LaidMember>,
    pub methods: Vec<LaidMember>,
    pub attributes: Vec<Laid>,
}

/// The bytes of class `c` with pool `pool`, or why they cannot hold it.
pub fn bytes(pool: &Pool, c: &Class) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend(0xcafe_babe_u32.to_be_bytes());
    out.extend(0_u16.to_be_bytes());
    out.extend(MAJOR_VERSION.to_be_bytes());
    pool.write(&mut out).ok_or("more than 65,534 constants")?;
    for v in [c.access, c.this, c.superclass] {
        out.extend(v.to_be_bytes());
    }
    out.extend(count(c.interfaces.len())?);
    out.extend(c.interfaces.iter().flat_map(|i| i.to_be_bytes()));
    for members in [&c.fields, &c.methods] {
        out.extend(count(members.len())?);
        for m in members {
            for v in [m.access, m.name, m.descriptor] {
                out.extend(v.to_be_bytes());
            }
            attributes(&mut out, &m.attributes)?;
        }
    }
    attributes(&mut out, &c.attributes)?;
    Ok(out)
}

fn attributes(out: &mut Vec<u8>, attrs: &[Laid]) -> Result<(), String> {
    out.extend(count(attrs.len())?);
    for a in attrs {
        out.extend(a.index.to_be_bytes());
        let len_at = out.len();
        out.extend([0; 4]);
        out.extend_from_slice(&a.body);
        if let Some(nested) = &a.nested {
            attributes(out, nested)?;
        }
        let len = u32::try_from(out.len() - len_at - 4).map_err(|_| "an attribute over 4 GiB")?;
        out[len_at..len_at + 4].copy_from_slice(&len.to_be_bytes());
    }
    Ok(())
}

fn count(n: usize) -> Result<[u8; 2], String> {
    u16::try_from(n).map(u16::to_be_bytes).map_err(|_| format!("{n} items in a table of at most 65,535"))
}

/// The body of the `Code` attribute of `c` up to its attributes.
pub fn code(c: &Code) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(c.bytes.len() + 12);
    out.extend(c.max_stack.to_be_bytes());
    out.extend(c.max_locals.to_be_bytes());
    out.extend((c.bytes.len() as u32).to_be_bytes());
    out.extend(&c.bytes);
    out.extend(count(c.exceptions.len())?);
    out.extend(c.exceptions.iter().flatten().flat_map(|v| v.to_be_bytes()));
    Ok(out)
}
