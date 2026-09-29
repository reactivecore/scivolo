//! Interned simple names.

use rustc_hash::FxHashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Name(u32);

/// Where a lone surrogate `u` of a name is kept: private-use character `SURROGATE_BASE + u`.
const SURROGATE_BASE: u32 = 0x10_0000;

/// The text of a name given as UTF-16 units; lone surrogates stay distinct.
pub fn from_utf16(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .map(|c| {
            c.unwrap_or_else(|e| char::from_u32(SURROGATE_BASE + e.unpaired_surrogate() as u32).unwrap())
        })
        .collect()
}

/// The UTF-16 units of a name's text, the inverse of `from_utf16`.
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

#[derive(Default)]
pub struct Names {
    texts: Vec<Box<str>>,
    ids: FxHashMap<Box<str>, u32>,
}

impl Names {
    pub fn intern(&mut self, text: &str) -> Name {
        if let Some(&id) = self.ids.get(text) {
            return Name(id);
        }
        let id = self.texts.len() as u32;
        self.texts.push(text.into());
        self.ids.insert(text.into(), id);
        Name(id)
    }

    pub fn text(&self, name: Name) -> &str {
        &self.texts[name.0 as usize]
    }
}
