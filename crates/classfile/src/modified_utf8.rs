//! The JVM's modified UTF-8 (JVMS §4.4.7): `\0` in two bytes, supplementary characters as two
//! surrogates of three bytes each.

/// `s` in modified UTF-8.
pub fn encode(s: &[u16]) -> Vec<u8> {
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

/// The text of modified UTF-8 `bytes`, and whether it is exact: no malformed bytes and no
/// unpaired surrogates, which the text replaces by `U+FFFD`.
pub fn decode(bytes: &[u8]) -> (String, bool) {
    if bytes.iter().all(|&b| (0x01..0x80).contains(&b)) {
        return (String::from_utf8_lossy(bytes).into_owned(), true);
    }
    let cont = |i: usize| bytes.get(i).filter(|&&b| b & 0xc0 == 0x80).map(|&b| u16::from(b & 0x3f));
    let mut units = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = u16::from(bytes[i]);
        let (unit, n) = match (bytes[i], cont(i + 1), cont(i + 2)) {
            (0x01..=0x7f, ..) => (b, 1),
            (0xc0..=0xdf, Some(x), _) => ((b & 0x1f) << 6 | x, 2),
            (0xe0..=0xef, Some(x), Some(y)) => ((b & 0x0f) << 12 | x << 6 | y, 3),
            _ => return (String::from_utf8_lossy(bytes).into_owned(), false),
        };
        units.push(unit);
        i += n;
    }
    match String::from_utf16(&units) {
        Ok(s) => (s, true),
        Err(_) => (String::from_utf16_lossy(&units), false),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn decodes_what_it_encodes() {
        let s: Vec<u16> = "a\0é€😀".encode_utf16().collect();
        assert_eq!(decode(&encode(&s)), ("a\0é€😀".to_owned(), true));
        assert!(!decode(&encode(&[0xd800])).1);
        assert!(!decode(&[0xc0]).1);
    }
}
