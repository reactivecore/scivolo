// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/serialization/BinarySerializer.scala
//! Writes definitions as one NIR file, which `decode` reads back; interned nodes are written
//! once per file, each after the nodes it refers to.

mod defn;
mod inst;
mod object;
mod value;

use crate::nir::{Defn, Global, GlobalId, PosId, StrId};
use crate::pool::Pool;
use crate::prelude::{COMPAT, LENGTH, MAGIC, REVISION, SECTIONS, Section};
use crate::tags;
use rustc_hash::FxHashMap;

/// Bytes being written: one buffer per entry or section.
#[derive(Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn u8(&mut self, b: u8) {
        self.0.push(b);
    }

    pub fn bool(&mut self, b: bool) {
        self.0.push(b as u8);
    }

    pub fn be_u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn be_u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn uleb(&mut self, mut v: u64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                return self.0.push(byte);
            }
            self.0.push(byte | 0x80);
        }
    }

    pub fn sleb(&mut self, mut v: i64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if (v == 0 && byte & 0x40 == 0) || (v == -1 && byte & 0x40 != 0) {
                return self.0.push(byte);
            }
            self.0.push(byte | 0x80);
        }
    }
}

pub struct Writer<'a> {
    pub pool: &'a Pool,
    sections: [Vec<u8>; 8],
    /// Per interned section, the offset of each node written, by id.
    offsets: [FxHashMap<u32, u32>; 8],
}

impl<'a> Writer<'a> {
    pub fn new(pool: &'a Pool) -> Self {
        Writer { pool, sections: Default::default(), offsets: Default::default() }
    }

    /// The offset of interned node `id` in section `s`, written there first if new, after the
    /// nodes `write` refers to.
    fn interned(&mut self, s: Section, id: u32, write: impl FnOnce(&mut Self, &mut Out)) -> u32 {
        if let Some(&off) = self.offsets[s as usize].get(&id) {
            return off;
        }
        let mut entry = Out::default();
        write(self, &mut entry);
        let off = self.sections[s as usize].len() as u32;
        self.sections[s as usize].extend_from_slice(&entry.0);
        self.offsets[s as usize].insert(id, off);
        off
    }

    pub fn string(&mut self, out: &mut Out, id: StrId) {
        let off = self.interned(Section::Strings, id.0, |w, e| match w.pool.utf16.get(&id) {
            Some(units) => {
                e.uleb(units.len() as u64);
                units.iter().for_each(|&u| e.uleb(u as u64));
            }
            None => {
                let s = w.pool.str(id);
                if s.is_ascii() {
                    e.uleb(s.len() as u64);
                    e.0.extend_from_slice(s.as_bytes());
                } else {
                    let units: Vec<u16> = s.encode_utf16().collect();
                    e.uleb(units.len() as u64);
                    units.iter().for_each(|&u| e.uleb(u as u64));
                }
            }
        });
        out.uleb(off as u64);
    }

    pub fn global(&mut self, out: &mut Out, id: GlobalId) {
        let off = self.interned(Section::Globals, id.0, |w, e| match w.pool.global(id).clone() {
            Global::None => e.u8(tags::global::NONE),
            Global::Top(s) => {
                e.u8(tags::global::TOP);
                w.string(e, s);
            }
            Global::Member(owner, sig) => {
                e.u8(tags::global::MEMBER);
                w.global(e, owner);
                w.string(e, sig);
            }
        });
        out.uleb(off as u64);
    }

    pub fn position(&mut self, out: &mut Out, id: PosId) {
        let off = self.interned(Section::Positions, id.0, |w, e| {
            let pos = w.pool.pos(id).clone();
            w.string(e, pos.file);
            e.uleb(pos.line as u64);
            e.uleb(pos.column as u64);
        });
        out.uleb(off as u64);
    }

    /// The file of `defns`, in order; `none` is the pool's `Global::None`.
    pub fn file(mut self, defns: &[Defn], none: GlobalId, has_entry_points: bool) -> Vec<u8> {
        let mut index = Out::default();
        for d in defns {
            let mut entry = Out::default();
            self.defn(&mut entry, d);
            let off = self.sections[Section::Defns as usize].len();
            self.sections[Section::Defns as usize].extend_from_slice(&entry.0);
            self.global(&mut index, d.name);
            index.sleb(off as i64);
        }
        self.global(&mut index, none);
        index.sleb(-1);
        self.sections[Section::Offsets as usize] = index.0;
        let mut out = Out::default();
        out.be_u32(MAGIC);
        out.be_u32(COMPAT);
        out.be_u32(REVISION);
        let mut start = LENGTH;
        for s in SECTIONS {
            out.be_u32(start as u32);
            start += self.sections[s as usize].len();
        }
        out.bool(has_entry_points);
        for s in SECTIONS {
            out.0.extend_from_slice(&self.sections[s as usize]);
        }
        out.0
    }

    fn section(&mut self, s: Section) -> &mut Vec<u8> {
        &mut self.sections[s as usize]
    }
}
