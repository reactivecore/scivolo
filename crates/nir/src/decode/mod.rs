//! Decodes one NIR file into a shared `Pool`; definitions load lazily by offset.

mod defn;
mod inst;
mod value;

use crate::cursor::{Cursor, Result};
use crate::nir::{Defn, Global, GlobalId, Pos, PosId, StrId};
use crate::pool::Pool;
use crate::prelude::{Prelude, SECTIONS, Section};
use crate::tags;

const UNSEEN: u32 = u32::MAX;
const MAX_DEPTH: u32 = 256;

#[derive(Default)]
pub struct Caches {
    slots: [Vec<u32>; 8],
}

impl Caches {
    fn reset(&mut self, prelude: &Prelude) {
        for (slots, s) in self.slots.iter_mut().zip(SECTIONS) {
            slots.clear();
            if !matches!(s, Section::Offsets | Section::Defns | Section::Insts) {
                slots.resize(prelude.len(s), UNSEEN);
            }
        }
    }
}

pub struct Reader<'a> {
    c: Cursor<'a>,
    pub prelude: Prelude,
    caches: &'a mut Caches,
    pub pool: &'a mut Pool,
    depth: u32,
    /// Whether bodies keep their local names and lexical scopes.
    pub debug_info: bool,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], caches: &'a mut Caches, pool: &'a mut Pool) -> Result<Self> {
        let prelude = Prelude::read(data)?;
        caches.reset(&prelude);
        Ok(Self { c: Cursor::new(data), prelude, caches, pool, depth: 0, debug_info: true })
    }

    /// A reader of `data` whose `caches` an earlier reader of the same file filled.
    pub fn resume(data: &'a [u8], caches: &'a mut Caches, pool: &'a mut Pool) -> Result<Self> {
        let prelude = Prelude::read(data)?;
        Ok(Self { c: Cursor::new(data), prelude, caches, pool, depth: 0, debug_info: true })
    }

    fn tag(&mut self) -> Result<u8> {
        self.c.u8()
    }

    fn bad_tag<T>(&self, what: &str, tag: u8) -> Result<T> {
        self.c.error(format!("unknown {what} tag {tag}"))
    }

    fn seq<T>(&mut self, mut f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let n = self.c.len_prefixed()?;
        (0..n).map(|_| f(self)).collect()
    }

    fn opt<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<Option<T>> {
        if self.c.bool()? { f(self).map(Some) } else { Ok(None) }
    }

    fn interned(&mut self, s: Section, decode: fn(&mut Self) -> Result<u32>) -> Result<u32> {
        let off = self.c.uleb32()? as usize;
        let Some(&slot) = self.caches.slots[s as usize].get(off) else {
            return self.c.error(format!("{s:?} reference {off} out of bounds"));
        };
        if slot != UNSEEN {
            return Ok(slot);
        }
        let id = self.at(self.prelude.start(s) + off, |r| r.nested(decode))?;
        self.caches.slots[s as usize][off] = id;
        Ok(id)
    }

    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return self.c.error("nodes nested too deeply");
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    fn at<T>(&mut self, pos: usize, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let saved = self.c.pos;
        self.c.pos = pos;
        let r = f(self);
        self.c.pos = saved;
        r
    }

    pub fn string(&mut self) -> Result<StrId> {
        self.interned(Section::Strings, |r| {
            let n = r.c.len_prefixed()?;
            let mut units = Vec::with_capacity(n);
            for _ in 0..n {
                units.push(r.c.uleb32()? as u16);
            }
            if units.iter().all(|&u| u < 0x80) {
                let s: String = units.iter().map(|&u| u as u8 as char).collect();
                return Ok(r.pool.strings.intern(s.into_boxed_str()));
            }
            match String::from_utf16(&units) {
                Ok(s) => Ok(r.pool.strings.intern(s.into_boxed_str())),
                Err(_) => {
                    let key = format!("{}\u{0}utf16{units:x?}", String::from_utf16_lossy(&units));
                    let id = r.pool.strings.intern(key.into_boxed_str());
                    r.pool.utf16.insert(StrId(id), units.into_boxed_slice());
                    Ok(id)
                }
            }
        })
        .map(StrId)
    }

    pub fn global(&mut self) -> Result<GlobalId> {
        self.interned(Section::Globals, |r| {
            let g = match r.tag()? {
                tags::global::NONE => Global::None,
                tags::global::TOP => Global::Top(r.string()?),
                tags::global::MEMBER => Global::Member(r.global()?, r.string()?),
                t => return r.bad_tag("global", t),
            };
            Ok(r.pool.globals.intern(g))
        })
        .map(GlobalId)
    }

    pub fn position(&mut self) -> Result<PosId> {
        self.interned(Section::Positions, |r| {
            let pos = Pos { file: r.string()?, line: r.c.uleb32()?, column: r.c.uleb32()? };
            Ok(r.pool.positions.intern(pos))
        })
        .map(PosId)
    }

    pub fn index(&mut self) -> Result<Vec<(GlobalId, u32)>> {
        self.at(self.prelude.start(Section::Offsets), |r| {
            let mut out = Vec::new();
            loop {
                let g = r.global()?;
                let off = r.c.sleb32()?;
                if *r.pool.global(g) == Global::None {
                    return Ok(out);
                }
                if off < 0 || off as usize >= r.prelude.len(Section::Defns) {
                    return r.c.error(format!("definition offset {off} out of bounds"));
                }
                out.push((g, off as u32));
            }
        })
    }

    pub fn defn_at(&mut self, offset: u32) -> Result<Defn> {
        self.at(self.prelude.start(Section::Defns) + offset as usize, Self::defn)
    }

    pub fn all(&mut self) -> Result<Vec<Defn>> {
        self.index()?.into_iter().map(|(_, off)| self.defn_at(off)).collect()
    }
}
