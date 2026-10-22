//! The snapshot file: a header with the region's address and bytes, then the VM's tables, in
//! little-endian fields.

use std::ops::Range;

const MAGIC: &[u8; 8] = b"SCIVSNAP";
const VERSION: u32 = 1;

#[derive(Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    pub fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }

    /// `u32::MAX` for `None`.
    pub fn opt(&mut self, v: Option<u32>) {
        self.u32(v.unwrap_or(u32::MAX));
    }
}

pub struct In<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> In<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("the snapshot is truncated")?;
        let bytes = self.data.get(self.pos..end).ok_or("the snapshot is truncated")?;
        self.pos += n;
        Ok(bytes)
    }

    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4 bytes")))
    }

    pub fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8 bytes")))
    }

    pub fn str(&mut self) -> Result<&'a str, String> {
        let len = self.u32()? as usize;
        std::str::from_utf8(self.take(len)?).map_err(|e| e.to_string())
    }

    pub fn opt(&mut self) -> Result<Option<u32>, String> {
        Ok(Some(self.u32()?).filter(|&v| v != u32::MAX))
    }

    /// `n` items read by `item`.
    pub fn list<T>(
        &mut self,
        mut item: impl FnMut(&mut Self) -> Result<T, String>,
    ) -> Result<Vec<T>, String> {
        let n = self.u32()?;
        (0..n).map(|_| item(self)).collect()
    }
}

/// A snapshot file's parts: where its region starts, the region's bytes, the address of the
/// thread cell, and the tables.
pub struct Image<'a> {
    pub start: usize,
    pub region: &'a [u8],
    pub thread: usize,
    pub tables: In<'a>,
}

/// The start of a snapshot body: the region at `start` and the thread cell.
pub fn region(out: &mut Out, start: usize, region: &[u8], thread: usize) {
    out.u64(start as u64);
    out.u64(thread as u64);
    out.u64(region.len() as u64);
    out.0.extend_from_slice(region);
}

/// The file of snapshot `body`: the format and program version and a hash of the body first.
pub fn file(body: Out) -> Vec<u8> {
    let mut out = Out(MAGIC.to_vec());
    out.u32(VERSION);
    out.str(env!("CARGO_PKG_VERSION"));
    out.u64(hash(&body.0));
    out.0.extend_from_slice(&body.0);
    out.0
}

fn hash(data: &[u8]) -> u64 {
    let words = data.chunks(8).map(|c| c.iter().rev().fold(0u64, |w, &b| w << 8 | b as u64));
    words.fold(data.len() as u64, |h, w| (h ^ w).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(23))
}

pub fn parse(data: &[u8]) -> Result<Image<'_>, String> {
    let mut input = In { data, pos: 0 };
    if input.take(8)? != MAGIC || input.u32()? != VERSION || input.str()? != env!("CARGO_PKG_VERSION") {
        return Err("not a snapshot of this version".into());
    }
    if input.u64()? != hash(&data[input.pos..]) {
        return Err("the snapshot is corrupt".into());
    }
    let start = input.u64()? as usize;
    let thread = input.u64()? as usize;
    let len = input.u64()? as usize;
    let region = input.take(len)?;
    match holds(&(start..start.saturating_add(len)), thread, 16) {
        true => Ok(Image { start, region, thread, tables: input }),
        false => Err("the thread cell is outside the region".into()),
    }
}

/// Whether `size` bytes at `addr`, 8-byte aligned, lie inside `range`.
pub fn holds(range: &Range<usize>, addr: usize, size: usize) -> bool {
    addr.is_multiple_of(8) && addr >= range.start && addr.checked_add(size).is_some_and(|e| e <= range.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_round_trip_and_corruption_is_found() {
        let bytes = [3u8; 32];
        let mut body = Out::default();
        region(&mut body, 0x2000_0000_0000, &bytes, 0x2000_0000_0010);
        body.str("tables");
        body.opt(None);
        let mut data = file(body);
        let mut image = parse(&data).unwrap();
        assert_eq!(
            (image.start, image.region, image.thread),
            (0x2000_0000_0000, &bytes[..], 0x2000_0000_0010)
        );
        assert_eq!((image.tables.str(), image.tables.opt()), (Ok("tables"), Ok(None)));
        assert!(image.tables.u8().is_err());
        let last = data.len() - 1;
        data[last] ^= 1;
        assert!(parse(&data).is_err());
        assert!(parse(&data[..20]).is_err());
        let mut body = Out::default();
        region(&mut body, 0x2000_0000_0000, &bytes, 0x2000_0000_0018);
        assert!(parse(&file(body)).is_err());
    }

    #[test]
    fn addresses_must_lie_inside_the_region() {
        let region = 0x1000..0x1020;
        assert!(holds(&region, 0x1018, 8) && holds(&region, 0x1000, 32));
        assert!(!holds(&region, 0x1020, 8) && !holds(&region, 0x1004, 8) && !holds(&region, 0x0ff8, 8));
        assert!(!holds(&region, usize::MAX - 7, 8));
    }
}
