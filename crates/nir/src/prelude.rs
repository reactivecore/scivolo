//! The 45-byte file header: magic, version and the eight section offsets.

use crate::cursor::{Cursor, Result};

pub const MAGIC: u32 = 0x2e4e_4952;
pub const COMPAT: u32 = 6;
pub const REVISION: u32 = 11;
pub const LENGTH: usize = 45;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Offsets,
    Strings,
    Positions,
    Globals,
    Types,
    Defns,
    Vals,
    Insts,
}

pub const SECTIONS: [Section; 8] = {
    use Section::*;
    [Offsets, Strings, Positions, Globals, Types, Defns, Vals, Insts]
};

#[derive(Clone, Copy, Debug)]
pub struct Prelude {
    pub compat: u32,
    pub revision: u32,
    pub starts: [usize; 8],
    pub ends: [usize; 8],
    pub has_entry_points: bool,
}

impl Prelude {
    pub fn read(data: &[u8]) -> Result<Prelude> {
        let mut c = Cursor::new(data);
        if c.be_u32()? != MAGIC {
            return c.error("not a NIR file (bad magic)");
        }
        let (compat, revision) = (c.be_u32()?, c.be_u32()?);
        if compat != COMPAT || revision > REVISION {
            return c.error(format!("unsupported NIR version {compat}.{revision}"));
        }
        let mut starts = [0usize; 8];
        for s in &mut starts {
            *s = c.be_u32()? as usize;
        }
        let has_entry_points = c.bool()?;
        let mut ends = [data.len(); 8];
        ends[..7].copy_from_slice(&starts[1..]);
        if starts[0] < LENGTH || (0..8).any(|i| starts[i] > ends[i] || ends[i] > data.len()) {
            return c.error("section offsets out of order or out of bounds");
        }
        Ok(Prelude { compat, revision, starts, ends, has_entry_points })
    }

    pub fn start(&self, s: Section) -> usize {
        self.starts[s as usize]
    }

    pub fn len(&self, s: Section) -> usize {
        self.ends[s as usize] - self.starts[s as usize]
    }
}
