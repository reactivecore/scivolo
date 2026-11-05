// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//   compiler/src/scala/quoted/runtime/impl/printers/Extractors.scala
//   compiler/src/dotty/tools/dotc/core/Constants.scala
//! Constants of `Printer.TreeCode` (dotty's `printConstant`), the structure printers and
//! `TypeRepr.toString`.

use super::printed::Printed;
use super::{Code, REFUSED, Shown};
use syntax::ast::constant::Constant as C;
use tasty::{Const, escape};

impl Code<'_> {
    pub fn constant(&mut self, c: &C) -> Shown {
        let text = match c {
            C::Unit => "()".to_owned(),
            C::Null => "null".to_owned(),
            C::Boolean(b) => b.to_string(),
            C::Byte(n) => n.to_string(),
            C::Short(n) => n.to_string(),
            C::Int(n) => n.to_string(),
            C::Long(n) => format!("{n}L"),
            C::Float(x) => format!("{}f", java_decimal(*x as f64, format!("{x:?}")).ok_or(REFUSED)?),
            C::Double(x) => java_decimal(*x, format!("{x:?}")).ok_or(REFUSED)?,
            &C::Char(u) => return Ok(self.quoted_literal('\'', &[u])),
            C::String(units) => return Ok(self.quoted_literal('"', units)),
        };
        self.literal(&text);
        Ok(())
    }

    /// UTF-16 `units` escaped between `quote`s as a literal, lone surrogates as they are
    /// (dotty's `Chars.escapedString`).
    fn quoted_literal(&mut self, quote: char, units: &[u16]) {
        let mut text = Printed::default();
        text.push(quote);
        text.push_units(units, escape);
        text.push(quote);
        let before = self.out.len();
        self.literal(&text);
        let start = before + self.out[before..].find(text.as_str()).unwrap_or(0);
        self.out.take_lone(start, text);
    }
}

/// Constant `c` as `Printer.TreeStructure` shows it (dotty's `Extractors.visitConstant`),
/// appended to `out`.
pub fn constant_structure(c: &C, out: &mut Printed) -> Result<(), &'static str> {
    let text = match c {
        C::Unit => "UnitConstant()".to_owned(),
        C::Null => "NullConstant()".to_owned(),
        C::Boolean(b) => format!("BooleanConstant({b})"),
        C::Byte(n) => format!("ByteConstant({n})"),
        C::Short(n) => format!("ShortConstant({n})"),
        C::Int(n) => format!("IntConstant({n})"),
        C::Long(n) => format!("LongConstant({n}L)"),
        C::Float(x) => {
            format!("FloatConstant({}f)", java_decimal(*x as f64, format!("{x:?}")).ok_or(REFUSED)?)
        }
        C::Double(x) => format!("DoubleConstant({}d)", java_decimal(*x, format!("{x:?}")).ok_or(REFUSED)?),
        &C::Char(u) => return Ok(raw(out, "CharConstant('", &[u], "')")),
        C::String(units) => return Ok(raw(out, "StringConstant(\"", units, "\")")),
    };
    out.push_str(&text);
    Ok(())
}

/// UTF-16 `units` unescaped between `open` and `close`.
fn raw(out: &mut Printed, open: &str, units: &[u16], close: &str) {
    out.push_str(open);
    out.push_units(units, |s, c| s.push(c));
    out.push_str(close);
}

/// The value of the constant of `tag` and `bits` as dotty's `Constant.toString` shows it.
pub fn constant_value(tag: u8, bits: i64) -> Option<String> {
    Some(match tasty::constant(tag, bits)? {
        Const::Unit => "()".to_owned(),
        Const::Bool(b) => b.to_string(),
        Const::Null => "null".to_owned(),
        Const::Byte(n) => n.to_string(),
        Const::Short(n) => n.to_string(),
        Const::Char(_) => char::from_u32(bits as u32)?.to_string(),
        Const::Int(n) => n.to_string(),
        Const::Long(n) => n.to_string(),
        Const::Float(x) => java_decimal(x as f64, format!("{x:?}"))?,
        Const::Double(x) => java_decimal(x, format!("{x:?}"))?,
        Const::Str(_) => return None,
    })
}

/// `x`, whose shortest decimal is `shortest`, as Java's `toString` writes it, for the values
/// written without an exponent.
pub fn java_decimal(x: f64, shortest: String) -> Option<String> {
    if x.is_nan() {
        return Some("NaN".into());
    }
    if x.is_infinite() {
        return Some(if x > 0.0 { "Infinity" } else { "-Infinity" }.into());
    }
    (x == 0.0 || (1e-3..1e7).contains(&x.abs())).then_some(shortest)
}

#[cfg(test)]
mod tests {
    use super::{C, Printed, constant_structure, constant_value};
    use tasty::tags::CHARconst;

    fn structure(c: &C) -> Vec<u16> {
        let mut out = Printed::default();
        constant_structure(c, &mut out).unwrap();
        out.into_utf16()
    }

    #[test]
    fn lone_surrogates_print_as_they_are() {
        let utf16 = |s: &str| s.encode_utf16().collect::<Vec<_>>();
        let mut char_constant = utf16("CharConstant('");
        char_constant.extend([0xd800, 0x27, 0x29]);
        assert_eq!(structure(&C::Char(0xd800)), char_constant);
        let mut string_constant = utf16("StringConstant(\"a");
        string_constant.extend([0xdc00, 0x22, 0x29]);
        assert_eq!(structure(&C::String(vec![0x61, 0xdc00].into())), string_constant);
        assert_eq!(structure(&C::String(vec![0xd83d, 0xde00].into())), utf16("StringConstant(\"😀\")"));
        assert!(constant_value(CHARconst, 0xd800).is_none());
    }
}
