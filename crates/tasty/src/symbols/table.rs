//! The symbol table a library image keeps per TASTy file: the file's entries, so that loading it
//! enters its symbols without decoding its trees.
//!
//! Layout, little-endian: `u32` version, `u8` scala2, `u32` entry count, then per entry `u8` kind
//! (0 for a package clause, else 1 + its index in `KINDS`), `u32` owner index (`u32::MAX` for
//! none), `u32` tree, `u64` flags, `u8` bits (1 extern, 2 derived, 4 has a target), the name, and
//! the target if it has one; a string is its `u32` length and its UTF-8 bytes.

use super::entries::{Entry, entries};
use super::loaded::Decoded;
use super::{Flags, Kind};
use crate::NodeId;

/// Changes whenever the layout or what `entries` computes changes; images are keyed by it.
pub const SYMBOL_TABLE_VERSION: u32 = 2;

const KINDS: [Kind; 6] = [Kind::Class, Kind::Type, Kind::TypeParam, Kind::Param, Kind::Val, Kind::Def];

/// The symbol table of TASTy file `data`; empty if it does not decode.
pub fn symbol_table(data: &[u8]) -> Vec<u8> {
    let Ok(file) = Decoded::parse(data) else { return Vec::new() };
    encode(file.scala2, &entries(&file))
}

pub fn encode(scala2: bool, entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&SYMBOL_TABLE_VERSION.to_le_bytes());
    out.push(scala2 as u8);
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        out.push(e.kind.and_then(|k| KINDS.iter().position(|&c| c == k)).map_or(0, |i| i as u8 + 1));
        out.extend_from_slice(&e.owner.unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(&e.tree.0.to_le_bytes());
        out.extend_from_slice(&e.flags.0.to_le_bytes());
        out.push(e.is_extern as u8 | (e.derived as u8) << 1 | (e.target.is_some() as u8) << 2);
        for s in std::iter::once(&e.name).chain(&e.target) {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
    }
    out
}

/// Whether the file is of the Scala 2 standard library, and its entries; `None` for a table of
/// another version or a malformed one.
pub fn decode(table: &[u8]) -> Option<(bool, Vec<Entry>)> {
    let mut r = Reader(table);
    if r.u32()? != SYMBOL_TABLE_VERSION {
        return None;
    }
    let scala2 = r.u8()? != 0;
    let count = r.u32()?;
    let mut out = Vec::with_capacity(count.min(1 << 16) as usize);
    for i in 0..count {
        let kind = match r.u8()? {
            0 => None,
            k => Some(*KINDS.get(k as usize - 1)?),
        };
        let owner = Some(r.u32()?).filter(|&o| o != u32::MAX);
        if owner.is_some_and(|o| o >= i) {
            return None;
        }
        let (tree, flags, bits) = (NodeId(r.u32()?), Flags(r.u64()?), r.u8()?);
        let name = r.string()?;
        let target = if bits & 4 != 0 { Some(r.string()?) } else { None };
        let (is_extern, derived) = (bits & 1 != 0, bits & 2 != 0);
        out.push(Entry { kind, name, owner, flags, target, is_extern, derived, tree });
    }
    r.0.is_empty().then_some((scala2, out))
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|[b]| b)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Option<u64> {
        self.take().map(u64::from_le_bytes)
    }

    fn string(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        let bytes = self.0.get(..len)?;
        self.0 = &self.0[len..];
        String::from_utf8(bytes.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_survive_a_round_trip() {
        let bytes =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/Circle.tasty")).unwrap();
        let file = Decoded::parse(&bytes).unwrap();
        let table = symbol_table(&bytes);
        assert_eq!(decode(&table), Some((file.scala2, entries(&file))));
        assert_eq!(decode(&table[..table.len() - 1]), None);
    }
}
