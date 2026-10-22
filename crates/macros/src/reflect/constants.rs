// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The constants of `quotes.reflect`: a kind, the two words of the value and a text.

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;
use syntax::ast::constant::Constant;
use vm::Reflected;

impl Host {
    /// Runs constant operation `op` on `args` and `text`.
    pub fn run_constant(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let arg = |i: usize| arg(args, i);
        match op {
            "constant" => {
                let t = self.tree(arg(0)? as u32)?;
                let c =
                    self.reflect().constant(t).ok_or("Literal.constant of a term other than a literal")?;
                Ok(encoded(c))
            }
            "literal" => {
                let c = decoded(arg(0)?, (arg(1)?, arg(2)?), text)?;
                let t = self.reflect().literal(c)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }
}

/// Constant `c` as its kind, its value's two words and its text.
pub fn encoded(c: Constant) -> Reflected {
    let words = |bits: u64| vec![bits as u32 as i32, (bits >> 32) as u32 as i32];
    let (kind, bits) = match c {
        Constant::Int(i) => (1, i as i64 as u64),
        Constant::Long(l) => (2, l as u64),
        Constant::Boolean(b) => (3, b as u64),
        Constant::Char(c) => (4, c as u64),
        Constant::Double(d) => (5, d.to_bits()),
        Constant::Float(f) => (6, f.to_bits() as u64),
        Constant::String(ref s) => return (vec![7, 0, 0], Some(String::from_utf16_lossy(s))),
        Constant::Null => (8, 0),
        Constant::Unit => (9, 0),
        Constant::Byte(b) => (10, b as i64 as u64),
        Constant::Short(s) => (11, s as i64 as u64),
    };
    ([vec![kind], words(bits)].concat(), None)
}

/// The constant of kind `kind` with value words `lo` and `hi` and text `text`.
fn decoded(kind: i32, (lo, hi): (i32, i32), text: &str) -> Result<Constant, &'static str> {
    let bits = (lo as u32 as u64) | ((hi as u32 as u64) << 32);
    Ok(match kind {
        1 => Constant::Int(bits as i32),
        2 => Constant::Long(bits as i64),
        3 => Constant::Boolean(bits != 0),
        4 => Constant::Char(bits as u16),
        5 => Constant::Double(f64::from_bits(bits)),
        6 => Constant::Float(f32::from_bits(bits as u32)),
        7 => Constant::String(text.encode_utf16().collect()),
        8 => Constant::Null,
        9 => Constant::Unit,
        10 => Constant::Byte(bits as i8),
        11 => Constant::Short(bits as i16),
        _ => return Err("a constant of an unknown kind"),
    })
}
