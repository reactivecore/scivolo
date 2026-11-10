//! The names that types refer to and declarations are looked up by, interned: texts one after another in one buffer.

use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;
use std::hash::BuildHasher;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NameId(u32);

pub struct NameTable {
    buf: String,
    /// Where each name's text starts in `buf`, then where the last one ends.
    bounds: Vec<u32>,
    ids: HashTable<u32>,
}

impl Default for NameTable {
    fn default() -> Self {
        NameTable { buf: String::new(), bounds: vec![0], ids: HashTable::new() }
    }
}

fn hash(text: &str) -> u64 {
    FxBuildHasher.hash_one(text)
}

fn text_of<'a>(buf: &'a str, bounds: &[u32], id: u32) -> &'a str {
    &buf[bounds[id as usize] as usize..bounds[id as usize + 1] as usize]
}

impl NameTable {
    pub fn intern(&mut self, text: &str) -> NameId {
        let h = hash(text);
        let NameTable { buf, bounds, ids } = self;
        if let Some(&id) = ids.find(h, |&id| text_of(buf, bounds, id) == text) {
            return NameId(id);
        }
        buf.push_str(text);
        let id = u32::try_from(bounds.len() - 1).expect("fewer than 2^32 names");
        bounds.push(u32::try_from(buf.len()).expect("names of fewer than 4 GiB"));
        ids.insert_unique(h, id, |&id| hash(text_of(buf, bounds, id)));
        NameId(id)
    }

    pub fn get(&self, name: NameId) -> &str {
        text_of(&self.buf, &self.bounds, name.0)
    }
}

#[cfg(test)]
mod tests {
    use super::NameTable;

    #[test]
    fn interning_is_injective() {
        let mut names = NameTable::default();
        let (a, b, e) = (names.intern("a"), names.intern("b"), names.intern(""));
        assert_eq!((names.intern("a"), names.intern("b"), names.intern("")), (a, b, e));
        assert_ne!(a, b);
        assert_eq!((names.get(a), names.get(b), names.get(e)), ("a", "b", ""));
    }
}
