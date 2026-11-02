// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/TastyString.scala
//! The TASTy of a quote as the string literal `unpickleExprV2` takes (dotty's
//! `TastyString.pickle`): Base64.

use crate::Lower;
use typer::tpd::TNodeId;

/// The longest string literal of a class file.
const MAX_STRING_SIZE: usize = 65535;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The literal of `bytes` in Base64; `Err` if they need several strings.
pub fn pickled(l: &mut Lower, bytes: &[u8]) -> Result<TNodeId, ()> {
    let text = encode(bytes);
    if text.len() > MAX_STRING_SIZE {
        return Err(());
    }
    Ok(l.string_literal(&text))
}

/// `bytes` in Base64 with padding.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            match i <= chunk.len() {
                true => out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char),
                false => out.push('='),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_base64() {
        assert_eq!(encode(&[0x5C, 0xA1, 0xAB, 0x1F]), "XKGrHw==");
        assert_eq!(encode(b"Man"), "TWFu");
        assert_eq!(encode(b"Ma"), "TWE=");
    }
}
