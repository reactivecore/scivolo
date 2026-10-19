// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   tasty/src/dotty/tools/tasty/TastyReader.scala
use std::fmt;

#[derive(Debug, Clone)]
pub struct Error {
    pub pos: usize,
    pub msg: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: {}", self.pos, self.msg)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Reader over TASTy micro-syntax: Nat/LongInt are big-endian base-128 with the stop bit (0x80) on the last byte.
#[derive(Clone)]
pub struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Cursor { bytes, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    pub fn err<T>(&self, msg: impl Into<String>) -> Result<T> {
        Err(Error { pos: self.pos, msg: msg.into() })
    }

    pub fn peek(&self) -> Result<u8> {
        match self.bytes.get(self.pos) {
            Some(&b) => Ok(b),
            None => self.err("unexpected end of input"),
        }
    }

    pub fn byte(&mut self) -> Result<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let Some(s) = self.bytes.get(self.pos..self.pos + n) else {
            return self.err(format!("need {n} bytes"));
        };
        self.pos += n;
        Ok(s)
    }

    pub fn nat(&mut self) -> Result<u64> {
        let start = self.pos;
        let mut x = 0u64;
        loop {
            let b = self.byte()?;
            x = (x << 7) | u64::from(b & 0x7f);
            if b & 0x80 != 0 {
                break;
            }
        }
        if self.pos - start > 9 {
            return self.err("nat too long");
        }
        Ok(x)
    }

    pub fn nat32(&mut self) -> Result<u32> {
        let n = self.nat()?;
        u32::try_from(n).or_else(|_| self.err(format!("nat {n} exceeds 32 bits")))
    }

    pub fn long_int(&mut self) -> Result<i64> {
        let start = self.pos;
        let mut b = self.byte()?;
        let mut x = i64::from(((b << 1) as i8) >> 1);
        while b & 0x80 == 0 {
            b = self.byte()?;
            x = (x << 7) | i64::from(b & 0x7f);
        }
        if self.pos - start > 10 {
            return self.err("long int too long");
        }
        Ok(x)
    }

    pub fn utf8(&mut self) -> Result<&'a str> {
        let n = self.nat32()? as usize;
        let at = self.pos;
        let raw = self.bytes(n)?;
        std::str::from_utf8(raw).map_err(|e| Error { pos: at, msg: e.to_string() })
    }

    /// Reads a Length and returns the absolute end position it implies.
    pub fn end(&mut self) -> Result<usize> {
        let n = self.nat32()? as usize;
        let end = self.pos + n;
        if end > self.bytes.len() {
            return self.err(format!("length {n} runs past end"));
        }
        Ok(end)
    }
}
