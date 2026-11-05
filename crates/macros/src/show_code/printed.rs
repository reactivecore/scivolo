//! Printed text that holds the lone surrogates a `String` cannot.

use std::ops::{Deref, DerefMut};

/// Text with lone surrogates, each at a byte offset of `text`.
#[derive(Default)]
pub struct Printed {
    text: String,
    lone: Vec<(usize, u16)>,
}

impl Printed {
    /// Appends UTF-16 `units`, each character written by `write`, each lone surrogate as is.
    pub fn push_units(&mut self, units: &[u16], write: fn(&mut String, char)) {
        for c in char::decode_utf16(units.iter().copied()) {
            match c {
                Ok(c) => write(&mut self.text, c),
                Err(e) => self.lone.push((self.text.len(), e.unpaired_surrogate())),
            }
        }
    }

    /// Takes the lone surrogates of `other`, whose text was appended at byte `base`.
    pub fn take_lone(&mut self, base: usize, other: Printed) {
        self.lone.extend(other.lone.into_iter().map(|(at, u)| (base + at, u)));
    }

    pub fn into_utf16(self) -> Vec<u16> {
        let (mut out, mut from) = (Vec::with_capacity(self.text.len()), 0);
        for (at, unit) in self.lone {
            out.extend(self.text[from..at].encode_utf16());
            out.push(unit);
            from = at;
        }
        out.extend(self.text[from..].encode_utf16());
        out
    }
}

impl Deref for Printed {
    type Target = String;

    fn deref(&self) -> &String {
        &self.text
    }
}

impl DerefMut for Printed {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.text
    }
}

#[cfg(test)]
mod tests {
    use super::Printed;

    #[test]
    fn lone_surrogates_keep_their_place() {
        let mut u = Printed::default();
        u.push('a');
        u.push_units(&[0x62, 0xd800, 0xd83d, 0xde00, 0xdc00], |s, c| s.push(c));
        let mut v = Printed::default();
        v.push('<');
        v.push_str(&u);
        v.take_lone(1, u);
        assert_eq!(v.into_utf16(), [0x3c, 0x61, 0x62, 0xd800, 0xd83d, 0xde00, 0xdc00]);
    }
}
