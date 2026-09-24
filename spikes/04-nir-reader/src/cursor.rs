//! Bounds-checked big-endian and LEB128 reads over one NIR file.

use std::fmt;

#[derive(Debug)]
pub struct Error {
    pub offset: usize,
    pub msg: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "at byte {}: {}", self.offset, self.msg)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Cursor<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn error<T>(&self, msg: impl Into<String>) -> Result<T> {
        Err(Error { offset: self.pos, msg: msg.into() })
    }

    pub fn u8(&mut self) -> Result<u8> {
        let Some(&b) = self.data.get(self.pos) else { return self.error("unexpected end of file") };
        self.pos += 1;
        Ok(b)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let Some(s) = self.data.get(self.pos..self.pos.saturating_add(n)) else {
            return self.error("unexpected end of file");
        };
        self.pos += n;
        Ok(s)
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn be_u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    pub fn be_u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
    }

    fn leb(&mut self, max_bytes: u32) -> Result<(u64, u32, u8)> {
        let (mut result, mut shift) = (0u64, 0u32);
        for _ in 0..max_bytes {
            let byte = self.u8()?;
            result |= u64::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                return Ok((result, shift, byte));
            }
        }
        self.error("invalid LEB128 sequence")
    }

    pub fn uleb32(&mut self) -> Result<u32> {
        Ok(self.leb(5)?.0 as u32)
    }

    pub fn uleb64(&mut self) -> Result<u64> {
        Ok(self.leb(10)?.0)
    }

    pub fn sleb32(&mut self) -> Result<i32> {
        let (v, shift, last) = self.leb(5)?;
        let mut v = v as u32 as i32;
        if shift < 32 && last & 0x40 != 0 {
            v |= -1i32 << shift;
        }
        Ok(v)
    }

    pub fn sleb64(&mut self) -> Result<i64> {
        let (v, shift, last) = self.leb(10)?;
        let mut v = v as i64;
        if shift < 64 && last & 0x40 != 0 {
            v |= -1i64 << shift;
        }
        Ok(v)
    }

    pub fn len_prefixed(&mut self) -> Result<usize> {
        let n = self.uleb32()? as usize;
        if n > self.data.len() - self.pos.min(self.data.len()) {
            return self.error(format!("sequence length {n} exceeds file"));
        }
        Ok(n)
    }
}
