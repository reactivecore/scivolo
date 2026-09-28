//! The member table of a NIR file in the library image: its definitions' offsets by signature
//! and which are fields, so a file's definitions are found without decoding its index.
//!
//! Layout, little-endian: `u32` offset of the top-level definition (`u32::MAX`: none), `u32`
//! count, per member sorted by signature `u32` signature start, `u32` signature length, `u32`
//! definition offset, `u32` 1 for a field; per member in file order its position in the sorted
//! records; then the signatures.

use nir::{Caches, Global, Pool, Reader};

const RECORD: usize = 16;

/// The table of NIR file `data` defining top-level `top`, or `None` if it defines a member of
/// another owner.
pub fn build(data: &[u8], top: &str) -> Option<Vec<u8>> {
    let (mut pool, mut caches) = (Pool::default(), Caches::default());
    let mut reader = Reader::new(data, &mut caches, &mut pool).ok()?;
    let mut top_offset = u32::MAX;
    let mut members = Vec::new();
    for (g, offset) in reader.index().ok()? {
        let owner = match *reader.pool.global(g) {
            Global::Member(owner, sig) => {
                let field = reader.var_at(offset).ok()?.is_some();
                members.push((reader.pool.str(sig).to_owned(), offset, field));
                owner
            }
            _ => {
                top_offset = offset;
                g
            }
        };
        if !matches!(*reader.pool.global(owner), Global::Top(name) if reader.pool.str(name) == top) {
            return None;
        }
    }
    let mut sorted: Vec<usize> = (0..members.len()).collect();
    sorted.sort_by(|&a, &b| members[a].0.cmp(&members[b].0));
    let mut position = vec![0u32; members.len()];
    for (at, &m) in sorted.iter().enumerate() {
        position[m] = at as u32;
    }
    let mut out = Vec::new();
    let mut strings = 8 + members.len() * (RECORD + 4);
    out.extend_from_slice(&top_offset.to_le_bytes());
    out.extend_from_slice(&(members.len() as u32).to_le_bytes());
    for &m in &sorted {
        let (sig, offset, field) = &members[m];
        for word in [strings as u32, sig.len() as u32, *offset, *field as u32] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        strings += sig.len();
    }
    position.iter().for_each(|p| out.extend_from_slice(&p.to_le_bytes()));
    sorted.iter().for_each(|&m| out.extend_from_slice(members[m].0.as_bytes()));
    Some(out)
}

/// A member table in the mapped library image.
pub struct Table(pub &'static [u8]);

impl Table {
    fn word(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.0[at..at + 4].try_into().expect("4 bytes"))
    }

    fn len(&self) -> usize {
        self.word(4) as usize
    }

    /// The signature, definition offset and field flag of the member at sorted position `i`.
    fn record(&self, i: usize) -> (&'static str, u32, bool) {
        let at = 8 + i * RECORD;
        let (start, len) = (self.word(at) as usize, self.word(at + 4) as usize);
        let sig = std::str::from_utf8(&self.0[start..start + len]).unwrap_or("");
        (sig, self.word(at + 8), self.word(at + 12) != 0)
    }

    /// The offset of the top-level definition, if the file has one.
    pub fn top(&self) -> Option<u32> {
        Some(self.word(0)).filter(|&o| o != u32::MAX)
    }

    /// The definition offset of the member with signature `sig`.
    pub fn find(&self, sig: &str) -> Option<u32> {
        let (mut low, mut high) = (0, self.len());
        while low < high {
            let mid = (low + high) / 2;
            let (s, offset, _) = self.record(mid);
            match s.cmp(sig) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => return Some(offset),
            }
        }
        None
    }

    /// Each member's signature, definition offset and field flag, in file order.
    pub fn members(&self) -> impl Iterator<Item = (&'static str, u32, bool)> + '_ {
        let order = 8 + self.len() * RECORD;
        (0..self.len()).map(move |i| self.record(self.word(order + i * 4) as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::{Table, build};
    use nir::{Caches, Global, Pool, Reader};

    #[test]
    fn table_matches_the_decoded_index() {
        let data = include_bytes!("../../nir/tests/fixtures/Employee.nir");
        let table = Table(build(data, "Employee").expect("a table").leak());
        let (mut pool, mut caches) = (Pool::default(), Caches::default());
        let mut reader = Reader::new(data, &mut caches, &mut pool).expect("a NIR file");
        let mut expected = Vec::new();
        for (g, offset) in reader.index().expect("an index") {
            match *reader.pool.global(g) {
                Global::Member(_, sig) => {
                    let field = reader.var_at(offset).expect("a definition").is_some();
                    expected.push((reader.pool.str(sig).to_owned(), offset, field));
                }
                _ => assert_eq!(table.top(), Some(offset)),
            }
        }
        let members: Vec<_> = table.members().map(|(s, o, f)| (s.to_owned(), o, f)).collect();
        assert_eq!(members, expected);
        assert!(expected.iter().any(|m| m.2));
        for (sig, offset, _) in &expected {
            assert_eq!(table.find(sig), Some(*offset));
        }
        assert_eq!(table.find("no such member"), None);
        assert!(build(data, "Other").is_none());
    }
}
