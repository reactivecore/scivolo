// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/PickleBuffer.scala
//   compiler/src/dotty/tools/dotc/core/classfile/ByteCodecs.scala
//! The bytes of a Scala 2 pickle: decoding them from the annotation's text and reading numbers.

/// The bytes of a pickle and the position read next.
pub struct PickleBuffer {
    pub bytes: Vec<u8>,
    pub at: usize,
}

/// The refusal of a pickle that ends inside an entry.
const TRUNCATED: &str = "a truncated Scala 2 pickle";

impl PickleBuffer {
    /// Reads one byte.
    pub fn byte(&mut self) -> Result<u8, String> {
        let b = *self.bytes.get(self.at).ok_or(TRUNCATED)?;
        self.at += 1;
        Ok(b)
    }

    /// Reads a natural number: base 128, most significant group first, the high bit set on all
    /// groups but the last (dotty's `readLongNat`).
    pub fn long_nat(&mut self) -> Result<u64, String> {
        let mut x = 0u64;
        loop {
            let b = self.byte()?;
            x = (x << 7) + u64::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(x);
            }
        }
    }

    /// Reads a natural number that fits an index (dotty's `readNat`).
    pub fn nat(&mut self) -> Result<usize, String> {
        Ok(self.long_nat()? as u32 as usize)
    }

    /// Reads a signed big-endian number of `len` bytes (dotty's `readLong`).
    pub fn long(&mut self, len: usize) -> Result<i64, String> {
        let mut x = 0u64;
        for _ in 0..len {
            x = (x << 8) | u64::from(self.byte()?);
        }
        let leading = 64u32.saturating_sub(len as u32 * 8);
        Ok(if leading >= 64 { 0 } else { ((x << leading) as i64) >> leading })
    }

    /// Reads the entry count and returns the start of each entry, skipping them (dotty's
    /// `createIndex`).
    pub fn index(&mut self) -> Result<Vec<usize>, String> {
        let count = self.nat()?;
        let mut index = Vec::with_capacity(count.min(self.bytes.len()));
        for _ in 0..count {
            index.push(self.at);
            self.byte()?;
            self.at += self.nat()?;
        }
        Ok(index)
    }
}

/// Decodes the text of a `ScalaSignature` annotation, one byte per character: `0` stands for
/// `0x7f` and every other byte for itself plus one, then the 7-bit groups are packed into bytes,
/// least significant first (dotty's `ByteCodecs.decode`).
pub fn decode(text: &[u8]) -> Vec<u8> {
    let (mut out, mut bits, mut acc) = (Vec::with_capacity(text.len() * 7 / 8), 0u32, 0u32);
    for &c in text {
        let group = if c == 0 { 0x7f } else { u32::from(c.wrapping_sub(1)) & 0x7f };
        acc |= group << bits;
        bits += 7;
        if bits >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            bits -= 8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{PickleBuffer, decode};

    #[test]
    fn natural_numbers_have_their_high_bit_set_but_on_the_last_group() {
        let mut b = PickleBuffer { bytes: vec![0x05, 0x81, 0x00, 0xff, 0x7f], at: 0 };
        assert_eq!(b.nat(), Ok(5));
        assert_eq!(b.nat(), Ok(128));
        assert_eq!(b.long_nat(), Ok(0x3fff));
        assert!(b.byte().is_err());
    }

    #[test]
    fn longs_are_sign_extended_from_their_length() {
        let mut b = PickleBuffer { bytes: vec![0xff, 0x01, 0x00, 0x80, 0x00], at: 0 };
        assert_eq!(b.long(1), Ok(-1));
        assert_eq!(b.long(2), Ok(256));
        assert_eq!(b.long(2), Ok(-32768));
    }

    #[test]
    fn the_index_holds_the_start_of_each_entry() {
        let mut b = PickleBuffer { bytes: vec![2, 1, 2, b'a', b'b', 3, 0], at: 0 };
        assert_eq!(b.index(), Ok(vec![1, 5]));
    }

    /// The text `ByteCodecs.encode` gives the pickle bytes `05 00 02`, and a zero character.
    #[test]
    fn the_annotation_text_decodes_to_the_pickle() {
        assert_eq!(decode(&[0x06, 0x01, 0x09, 0x01]), vec![0x05, 0x00, 0x02]);
        assert_eq!(decode(&[0x00, 0x01]), vec![0x7f]);
    }
}
