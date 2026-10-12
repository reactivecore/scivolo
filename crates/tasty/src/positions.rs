//! The Positions section: the span of each tree that pickled one, by its address
//! (dotty's `PositionUnpickler`).

use crate::cursor::{Cursor, Result};
use rustc_hash::FxHashMap;

/// The header marking a change of source file.
const SOURCE: i64 = 4;

/// A span in the source of a TASTy file: offsets in UTF-16 units, the point `None` for a
/// synthetic span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TastySpan {
    pub start: u32,
    pub end: u32,
    pub point: Option<u32>,
}

/// The spans of the trees, by address, of the Positions section `bytes`.
pub fn decode_positions(bytes: &[u8]) -> Result<FxHashMap<u32, TastySpan>> {
    let mut cur = Cursor::new(bytes);
    let lines = cur.nat()?;
    for _ in 0..lines {
        cur.nat()?;
    }
    let mut spans = FxHashMap::default();
    let (mut index, mut start, mut end) = (0i64, 0i64, 0i64);
    while !cur.at_end() {
        let header = cur.long_int()?;
        if header == SOURCE {
            cur.nat()?;
            continue;
        }
        index += header >> 3;
        if header & 4 != 0 {
            start += cur.long_int()?;
        }
        if header & 2 != 0 {
            end += cur.long_int()?;
        }
        let point = if header & 1 != 0 { Some(start + cur.long_int()?) } else { None };
        let offset = |o: i64| u32::try_from(o).or_else(|_| cur.err(format!("span offset {o}")));
        let span =
            TastySpan { start: offset(start)?, end: offset(end)?, point: point.map(offset).transpose()? };
        spans.insert(offset(index)?, span);
    }
    Ok(spans)
}
