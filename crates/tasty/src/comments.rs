// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/CommentUnpickler.scala
//! The Comments section: the raw doc comment of each definition that has one, by its address
//! (dotty's `CommentUnpickler`).

use crate::cursor::{Cursor, Result};
use rustc_hash::FxHashMap;

/// Decodes the raw doc comments, by the address of their definitions, of Comments section `bytes`.
pub fn decode_comments(bytes: &[u8]) -> Result<FxHashMap<u32, String>> {
    let mut cur = Cursor::new(bytes);
    let mut comments = FxHashMap::default();
    while !cur.at_end() {
        let addr = cur.nat32()?;
        let raw = cur.utf8()?.to_owned();
        cur.long_int()?;
        comments.insert(addr, raw);
    }
    Ok(comments)
}

#[cfg(test)]
mod tests {
    use super::decode_comments;

    #[test]
    fn reads_address_text_and_skips_span() {
        let bytes = [0x87, 0x82, b'/', b'*', 0x81];
        let comments = decode_comments(&bytes).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[&7], "/*");
    }
}
