//! The constant pool, each entry added once.

use rustc_hash::FxHashMap;

#[derive(Clone, PartialEq, Eq, Hash)]
enum Entry {
    Utf8(Box<[u16]>),
    Integer(i32),
    Class(u16),
    String(u16),
    Fieldref(u16, u16),
    Methodref(u16, u16),
    InterfaceMethodref(u16, u16),
    NameAndType(u16, u16),
}

/// The kind of a member reference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ref {
    Field,
    Method,
    InterfaceMethod,
}

#[derive(Default)]
pub struct Pool {
    entries: Vec<Entry>,
    index: FxHashMap<Entry, u16>,
}

impl Pool {
    fn add(&mut self, e: Entry) -> u16 {
        if let Some(&i) = self.index.get(&e) {
            return i;
        }
        self.entries.push(e.clone());
        let i = u16::try_from(self.entries.len()).unwrap_or(u16::MAX);
        self.index.insert(e, i);
        i
    }

    pub fn utf8(&mut self, s: &str) -> u16 {
        self.add(Entry::Utf8(s.encode_utf16().collect()))
    }

    pub fn integer(&mut self, v: i32) -> u16 {
        self.add(Entry::Integer(v))
    }

    /// The class of internal name `name`, e.g. `java/lang/Object` or `[I`.
    pub fn class(&mut self, name: &str) -> u16 {
        let name = self.utf8(name);
        self.add(Entry::Class(name))
    }

    /// The `String` constant of UTF-16 units `s`, lone surrogates included.
    pub fn string(&mut self, s: &[u16]) -> u16 {
        let s = self.add(Entry::Utf8(s.into()));
        self.add(Entry::String(s))
    }

    pub fn name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let (name, descriptor) = (self.utf8(name), self.utf8(descriptor));
        self.add(Entry::NameAndType(name, descriptor))
    }

    /// The reference to member `name` of class `owner` with `descriptor`.
    pub fn member(&mut self, kind: Ref, owner: &str, name: &str, descriptor: &str) -> u16 {
        let (owner, nt) = (self.class(owner), self.name_and_type(name, descriptor));
        self.add(match kind {
            Ref::Field => Entry::Fieldref(owner, nt),
            Ref::Method => Entry::Methodref(owner, nt),
            Ref::InterfaceMethod => Entry::InterfaceMethodref(owner, nt),
        })
    }

    /// The `constant_pool_count` and the entries, or `None` past 65,534 entries.
    pub fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        let count = u16::try_from(self.entries.len() + 1).ok().filter(|&c| c < u16::MAX)?;
        out.extend(count.to_be_bytes());
        for e in &self.entries {
            let (tag, a, b) = match *e {
                Entry::Utf8(ref s) => {
                    let bytes = modified_utf8(s);
                    out.push(1);
                    out.extend(u16::try_from(bytes.len()).ok()?.to_be_bytes());
                    out.extend(bytes);
                    continue;
                }
                Entry::Integer(v) => {
                    out.push(3);
                    out.extend(v.to_be_bytes());
                    continue;
                }
                Entry::Class(n) => (7, n, None),
                Entry::String(s) => (8, s, None),
                Entry::Fieldref(c, nt) => (9, c, Some(nt)),
                Entry::Methodref(c, nt) => (10, c, Some(nt)),
                Entry::InterfaceMethodref(c, nt) => (11, c, Some(nt)),
                Entry::NameAndType(n, d) => (12, n, Some(d)),
            };
            out.push(tag);
            out.extend(a.to_be_bytes());
            out.extend(b.into_iter().flat_map(u16::to_be_bytes));
        }
        Some(())
    }
}

/// `s` in the JVM's modified UTF-8: `\0` in two bytes, supplementary characters as two
/// surrogates of three bytes each.
fn modified_utf8(s: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &c in s {
        match c {
            0x01..=0x7f => out.push(c as u8),
            0..=0x7ff => out.extend([0xc0 | (c >> 6) as u8, 0x80 | (c & 0x3f) as u8]),
            _ => {
                out.extend([0xe0 | (c >> 12) as u8, 0x80 | ((c >> 6) & 0x3f) as u8, 0x80 | (c & 0x3f) as u8])
            }
        }
    }
    out
}
