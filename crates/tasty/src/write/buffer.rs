// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   tasty/src/dotty/tools/tasty/TastyBuffer.scala
//   tasty/src/dotty/tools/tasty/TastyHash.scala

/// The bytes of an address field reserved for a later reference.
pub const ADDR_WIDTH: usize = 4;

/// An index into a buffer's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Addr(pub usize);

impl Addr {
    pub fn relative_to(self, base: Addr) -> Addr {
        Addr(self.0 - base.0 - ADDR_WIDTH)
    }
}

/// The number of base-128 digits of `nat`.
pub fn nat_size(nat: usize) -> usize {
    let mut n = nat;
    let mut size = 1;
    while n >= 128 {
        n >>= 7;
        size += 1;
    }
    size
}

/// A byte buffer written in TASTy's micro-syntax, with addresses patched in place.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    pub bytes: Vec<u8>,
}

impl Buffer {
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn current_addr(&self) -> Addr {
        Addr(self.bytes.len())
    }

    pub fn write_byte(&mut self, b: u8) {
        self.bytes.push(b);
    }

    pub fn write_bytes(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }

    /// Big-endian base 128, the stop bit 0x80 on the last digit.
    pub fn write_nat(&mut self, x: u64) {
        let digits = (64 - x.leading_zeros()).div_ceil(7).max(1);
        for i in (1..digits).rev() {
            self.write_byte((x >> (7 * i)) as u8 & 0x7f);
        }
        self.write_byte(x as u8 & 0x7f | 0x80);
    }

    /// Two's complement big-endian base 128, the stop bit 0x80 on the last digit.
    pub fn write_long_int(&mut self, x: i64) {
        let mut digits = vec![x as u8 & 0x7f | 0x80];
        let mut v = x;
        while v >> 7 != -((v >> 6) & 1) {
            v >>= 7;
            digits.push(v as u8 & 0x7f);
        }
        digits.reverse();
        self.write_bytes(&digits);
    }

    pub fn write_uncompressed_long(&mut self, x: u64) {
        self.write_bytes(&x.to_be_bytes());
    }

    pub fn write_utf8(&mut self, s: &str) {
        self.write_nat(s.len() as u64);
        self.write_bytes(s.as_bytes());
    }

    /// Writes `x` right-adjusted into the `width` bytes at `at`.
    pub fn put_nat(&mut self, at: Addr, x: usize, width: usize) {
        let mut y = x;
        let mut digit = (y & 0x7f) as u8 | 0x80;
        for w in (0..width).rev() {
            self.bytes[at.0 + w] = digit;
            y >>= 7;
            digit = (y & 0x7f) as u8;
        }
        assert!(y == 0, "number {x} too large to fit in {width} bytes");
    }

    pub fn get_nat(&self, at: Addr) -> usize {
        let mut x = 0;
        for &b in &self.bytes[at.0..] {
            x = (x << 7) | (b & 0x7f) as usize;
            if b & 0x80 != 0 {
                break;
            }
        }
        x
    }

    /// The first address from `at` on that holds a non-zero byte.
    pub fn skip_zeroes(&self, at: Addr) -> Addr {
        Addr(at.0 + self.bytes[at.0..].iter().take_while(|&&b| b == 0).count())
    }

    /// The address after the natural number at `at`.
    pub fn skip_nat(&self, at: Addr) -> Addr {
        Addr(at.0 + 1 + self.bytes[at.0..].iter().take_while(|&&b| b & 0x80 == 0).count())
    }

    pub fn reserve_addr(&mut self) -> Addr {
        let at = self.current_addr();
        self.bytes.resize(at.0 + ADDR_WIDTH, 0);
        at
    }

    pub fn fill_addr(&mut self, at: Addr, target: Addr) {
        self.put_nat(at, target.0, ADDR_WIDTH);
    }

    pub fn write_addr(&mut self, addr: Addr) {
        self.write_nat(addr.0 as u64);
    }
}

/// The PJW hash of `data`, extended to 64 bits.
pub fn pjw_hash64(data: &[u8]) -> u64 {
    data.iter().fold(0u64, |h, &d| {
        let h = (h << 8) + d as u64;
        let high = h & 0xFF00_0000_0000_0000;
        (h ^ (high >> 48)) & !high
    })
}
