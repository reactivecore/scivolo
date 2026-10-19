// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/localopt/FormatChecker.scala
//! The conversion specifiers of `f` interpolation parts (dotty's `formatPattern` and
//! `Conversion` in `TypedFormatChecker`).

pub type Checked<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    String,
    Hash,
    Boolean,
    Character,
    Integral,
    FloatingPoint,
    DateTime,
    Literal,
    Error,
}

/// A conversion specifier `%[index$][flags][width][.precision]op` at `start` of its part.
pub struct Conversion {
    pub start: usize,
    pub index: Option<u32>,
    pub flags: String,
    pub width: Option<u32>,
    pub precision: Option<u32>,
    pub op: String,
    pub kind: Kind,
}

/// The conversions in `part`, in order.
pub fn conversions(part: &[u16]) -> Checked<Vec<Conversion>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = part[i..].iter().position(|&c| c == u16::from(b'%')) {
        let (cv, end) = conversion(part, i + p)?;
        out.push(cv);
        i = end;
    }
    Ok(out)
}

/// The ASCII character `part[i]`, if any.
fn at(part: &[u16], i: usize) -> Option<char> {
    part.get(i).and_then(|&c| u8::try_from(c).ok()).filter(u8::is_ascii).map(char::from)
}

/// The end of the run of characters from `i` that `ok` accepts.
fn run(part: &[u16], mut i: usize, ok: impl Fn(char) -> bool) -> usize {
    while at(part, i).is_some_and(&ok) {
        i += 1;
    }
    i
}

fn text(part: &[u16], from: usize, to: usize) -> String {
    part[from..to].iter().map(|&c| char::from(c as u8)).collect()
}

fn number(part: &[u16], from: usize, to: usize) -> Checked<Option<u32>> {
    if from == to {
        return Ok(None);
    }
    text(part, from, to).parse().map(Some).map_err(|_| "f interpolations with a number out of range")
}

/// The conversion matching `%(?:(\d+)\$)?([-#+ 0,(<]+)?(\d+)?(\.\d+)?([tT]?[%a-zA-Z])?` at `start`,
/// and its end.
fn conversion(part: &[u16], start: usize) -> Checked<(Conversion, usize)> {
    let digit = |c: char| c.is_ascii_digit();
    let mut i = start + 1;
    let digits = run(part, i, digit);
    let mut index = None;
    if digits > i && at(part, digits) == Some('$') {
        index = number(part, i, digits)?;
        i = digits + 1;
    }
    let flags_end = run(part, i, |c| "-#+ 0,(<".contains(c));
    let flags = text(part, i, flags_end);
    let width_end = run(part, flags_end, digit);
    let width = number(part, flags_end, width_end)?;
    i = width_end;
    let mut precision = None;
    if at(part, i) == Some('.') && at(part, i + 1).is_some_and(digit) {
        let end = run(part, i + 1, digit);
        precision = number(part, i + 1, end)?;
        i = end;
    }
    let op_char = |c: Option<char>| c.is_some_and(|c| c == '%' || c.is_ascii_alphabetic());
    let op_len = match at(part, i) {
        Some('t' | 'T') if op_char(at(part, i + 1)) => 2,
        c if op_char(c) => 1,
        _ => return Err("f interpolations with a missing conversion operator"),
    };
    let op = text(part, i, i + op_len);
    let kind = match op.as_bytes()[0] {
        b's' | b'S' => Kind::String,
        b'h' | b'H' => Kind::Hash,
        b'b' | b'B' => Kind::Boolean,
        b'c' | b'C' => Kind::Character,
        b'd' | b'o' | b'x' | b'X' => Kind::Integral,
        b'e' | b'E' | b'f' | b'g' | b'G' | b'a' | b'A' => Kind::FloatingPoint,
        b't' | b'T' => Kind::DateTime,
        b'%' | b'n' => Kind::Literal,
        _ => Kind::Error,
    };
    Ok((Conversion { start, index, flags, width, precision, op, kind }, i + op_len))
}

impl Conversion {
    pub fn has(&self, f: char) -> bool {
        self.flags.contains(f)
    }

    pub fn is_indexed(&self) -> bool {
        self.index.is_some() || self.has('<')
    }

    /// The conversion character (dotty's `cc`).
    pub fn cc(&self) -> char {
        let op = self.op.as_bytes();
        char::from(match self.kind {
            Kind::DateTime if op.len() <= 1 => b'?',
            Kind::DateTime => op[1],
            _ => op[0],
        })
    }
}
