//! Interned simple names.

use crate::token::{Token, keyword_at, keyword_spellings};
use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;
use std::hash::BuildHasher;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Name(u32);

/// Where a lone surrogate `u` of a name is kept: private-use character `SURROGATE_BASE + u`.
const SURROGATE_BASE: u32 = 0x10_0000;

/// The character that stands for lone surrogate `u` in text.
pub fn surrogate_char(u: u16) -> char {
    char::from_u32(SURROGATE_BASE + u as u32).expect("a private-use character")
}

/// Append the text of a name given as UTF-16 units to `out`; lone surrogates stay distinct.
pub fn push_utf16(out: &mut String, units: &[u16]) {
    let ascii = units.iter().position(|&u| u >= 0x80).unwrap_or(units.len());
    out.extend(units[..ascii].iter().map(|&u| u as u8 as char));
    out.extend(
        char::decode_utf16(units[ascii..].iter().copied())
            .map(|c| c.unwrap_or_else(|e| surrogate_char(e.unpaired_surrogate()))),
    );
}

/// The UTF-16 units of a name's text, the inverse of `push_utf16`.
pub fn to_utf16(text: &str) -> Vec<u16> {
    let mut units = Vec::with_capacity(text.len());
    for c in text.chars() {
        match (c as u32).checked_sub(SURROGATE_BASE) {
            Some(u @ 0xD800..=0xDFFF) => units.push(u as u16),
            _ => units.extend_from_slice(c.encode_utf16(&mut [0; 2])),
        }
    }
    units
}

/// Whether `name` is the name of a variable pattern: it starts with a lower case letter or `_`.
pub fn is_var_pattern_name(name: &str) -> bool {
    use unicode_general_category::{GeneralCategory as G, get_general_category};
    let Some(first) = name.chars().next() else { return false };
    (first == '_' || get_general_category(first) == G::LowercaseLetter)
        && !matches!(name, "false" | "true" | "null")
}

/// The interned names of a compilation unit, their texts one after another in one buffer; the
/// keywords come first, in the order of `token::keyword_spellings`.
pub struct Names {
    buf: String,
    /// Where each name's text starts in `buf`, then where the last one ends.
    bounds: Vec<u32>,
    ids: HashTable<u32>,
}

impl Default for Names {
    fn default() -> Self {
        let mut bounds = Vec::with_capacity(512);
        bounds.push(0);
        let mut names =
            Names { buf: String::with_capacity(4096), bounds, ids: HashTable::with_capacity(512) };
        for text in keyword_spellings() {
            names.intern(text);
        }
        names
    }
}

impl Name {
    /// The keyword this name spells, if any.
    pub fn keyword(self) -> Option<Token> {
        keyword_at(self.0 as usize)
    }
}

fn hash(text: &str) -> u64 {
    FxBuildHasher.hash_one(text)
}

fn text_of<'a>(buf: &'a str, bounds: &[u32], id: u32) -> &'a str {
    &buf[bounds[id as usize] as usize..bounds[id as usize + 1] as usize]
}

impl Names {
    pub fn intern(&mut self, text: &str) -> Name {
        let h = hash(text);
        if let Some(&id) = self.ids.find(h, |&id| text_of(&self.buf, &self.bounds, id) == text) {
            return Name(id);
        }
        self.buf.push_str(text);
        self.add_tail(h)
    }

    /// Intern the name spelled by UTF-16 `units`, as `push_utf16` decodes them.
    pub fn intern_utf16(&mut self, units: &[u16]) -> Name {
        let start = self.buf.len();
        push_utf16(&mut self.buf, units);
        let Names { buf, bounds, ids } = self;
        let text = &buf[start..];
        let h = hash(text);
        if let Some(&id) = ids.find(h, |&id| text_of(buf, bounds, id) == text) {
            buf.truncate(start);
            return Name(id);
        }
        self.add_tail(h)
    }

    /// Add the text after the last name in `buf`, with hash `h`, as a new name.
    fn add_tail(&mut self, h: u64) -> Name {
        let Names { buf, bounds, ids } = self;
        let id = u32::try_from(bounds.len() - 1).expect("fewer than 2^32 names");
        bounds.push(u32::try_from(buf.len()).expect("names of fewer than 4 GiB"));
        ids.insert_unique(h, id, |&id| hash(text_of(buf, bounds, id)));
        Name(id)
    }

    pub fn text(&self, name: Name) -> &str {
        text_of(&self.buf, &self.bounds, name.0)
    }
}

/// The counters of a compilation unit's fresh names, one per prefix (dotty's `FreshNameCreator`).
#[derive(Default)]
pub struct FreshNames(rustc_hash::FxHashMap<String, u32>);

impl FreshNames {
    /// `prefix` followed by the next number for it: `_$1`, `evidence$2`.
    pub fn next(&mut self, names: &mut Names, prefix: &str) -> Name {
        let n = self.count(prefix);
        names.intern(&format!("{prefix}{n}"))
    }

    /// The next `$N$` (dotty's `UniqueName`).
    pub fn unique(&mut self, names: &mut Names) -> Name {
        let n = self.count("$");
        names.intern(&format!("${n}$"))
    }

    /// Raises each counter to at least that of `other`.
    pub fn include(&mut self, other: &FreshNames) {
        for (prefix, &n) in &other.0 {
            let mine = self.0.entry(prefix.clone()).or_insert(0);
            *mine = (*mine).max(n);
        }
    }

    fn count(&mut self, prefix: &str) -> u32 {
        let n = self.0.entry(prefix.to_owned()).or_insert(0);
        *n += 1;
        *n
    }
}

#[cfg(test)]
mod tests {
    use super::Names;
    use crate::token::{Token, keyword};

    #[test]
    fn keywords_have_their_ids() {
        let mut names = Names::default();
        for text in crate::token::keyword_spellings() {
            assert_eq!(names.intern(text).keyword(), keyword(text), "{text}");
        }
        assert_eq!(names.intern("type").keyword(), Some(Token::Type));
        assert_eq!(names.intern("foo").keyword(), None);
        assert_eq!(names.intern_utf16(&[b'v' as u16, b'a' as u16, b'l' as u16]).keyword(), Some(Token::Val));
        assert_eq!(names.intern_utf16(&[0xD800]), names.intern_utf16(&[0xD800]));
        assert_ne!(names.intern_utf16(&[0xD800]), names.intern_utf16(&[0xD801]));
    }
}
