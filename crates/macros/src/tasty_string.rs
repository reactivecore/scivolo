// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/TastyString.scala
//! The TASTy of a pickled quote from its string form (dotty's `TastyString.unpickle`): Base64.

use crate::host::Host;
use tasty::TypeId;

impl Host {
    /// The bytes of `pickled` and the types numbered `types`.
    pub fn decoded(&self, pickled: &str, types: &[u32]) -> Result<(Vec<u8>, Vec<TypeId>), &'static str> {
        let bytes = decode(pickled).ok_or("a quote not in Base64")?;
        let types: Result<Vec<TypeId>, _> = types.iter().map(|&t| self.type_of(t)).collect();
        Ok((bytes, types.map_err(|_| "a quote of an unknown type")?))
    }
}

/// The bytes Base64 text `text` encodes; `None` if it is not Base64.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        acc = (acc << 6 | value(c)? as u32) & 0xFFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_base64() {
        assert_eq!(decode("XKGrHw==").as_deref(), Some(&[0x5C, 0xA1, 0xAB, 0x1F][..]));
        assert_eq!(decode("TWFu").as_deref(), Some(&b"Man"[..]));
        assert_eq!(decode("TWE=").as_deref(), Some(&b"Ma"[..]));
        assert_eq!(decode("a b"), None);
    }
}
