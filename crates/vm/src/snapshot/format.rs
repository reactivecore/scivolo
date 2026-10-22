//! The snapshot file: a header with the region's address and bytes, then the VM's tables, in
//! little-endian fields.

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
        let bytes = self.data.get(self.pos..self.pos + n).ok_or("the snapshot is truncated")?;
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
/// thread cell, the packages of its classes as directories (`java/lang`), and the tables.
pub struct Image<'a> {
    pub start: usize,
    pub region: &'a [u8],
    pub thread: usize,
    pub packages: Vec<&'a str>,
    pub tables: In<'a>,
}

/// The start of a snapshot body: the region at `start`, the thread cell and the packages.
pub fn region(out: &mut Out, start: usize, region: &[u8], thread: usize, packages: &[String]) {
    out.u64(start as u64);
    out.u64(thread as u64);
    out.u64(region.len() as u64);
    out.0.extend_from_slice(region);
    out.u32(packages.len() as u32);
    packages.iter().for_each(|p| out.str(p));
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
    let packages = input.list(In::str)?;
    Ok(Image { start, region, thread, packages, tables: input })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_round_trip_and_corruption_is_found() {
        let mut body = Out::default();
        region(&mut body, 0x2000_0000_0000, &[1, 2, 3], 0x2000_0000_0010, &["java/lang".into()]);
        body.str("tables");
        body.opt(None);
        let mut data = file(body);
        let mut image = parse(&data).unwrap();
        assert_eq!(
            (image.start, image.region, image.thread),
            (0x2000_0000_0000, &[1, 2, 3][..], 0x2000_0000_0010)
        );
        assert_eq!(image.packages, ["java/lang"]);
        assert_eq!((image.tables.str(), image.tables.opt()), (Ok("tables"), Ok(None)));
        assert!(image.tables.u8().is_err());
        let last = data.len() - 1;
        data[last] ^= 1;
        assert!(parse(&data).is_err());
        assert!(parse(&data[..20]).is_err());
    }
}
