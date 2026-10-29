// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//   compiler/src/scala/quoted/runtime/impl/printers/Extractors.scala
//   compiler/src/dotty/tools/dotc/util/Chars.scala
//! Constants of `Printer.TreeCode` (dotty's `printConstant`, `Chars.escapedString`) and the
//! structure printers.

use super::{Code, REFUSED, Shown};
use syntax::ast::constant::Constant as C;

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
            &C::Char(u) => {
                let mut s = String::from("'");
                escape(&mut s, char::from_u32(u as u32).unwrap_or('\u{fffd}'));
                s.push('\'');
                s
            }
            C::String(units) => {
                let mut s = String::from("\"");
                for c in char::decode_utf16(units.iter().copied()) {
                    escape(&mut s, c.unwrap_or('\u{fffd}'));
                }
                s.push('"');
                s
            }
        };
        self.push(&text);
        Ok(())
    }
}

/// Constant `c` as `Printer.TreeStructure` shows it (dotty's `Extractors.visitConstant`).
pub fn constant_structure(c: &C) -> Result<String, &'static str> {
    Ok(match c {
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
        &C::Char(u) => format!("CharConstant('{}')", char::from_u32(u as u32).unwrap_or('\u{fffd}')),
        C::String(units) => format!("StringConstant(\"{}\")", String::from_utf16_lossy(units)),
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

/// Appends `c`, escaped as dotty's `Chars.escapedChar` does.
fn escape(s: &mut String, c: char) {
    let replace = match c {
        '\u{8}' => "\\b",
        '\t' => "\\t",
        '\n' => "\\n",
        '\u{c}' => "\\f",
        '\r' => "\\r",
        '"' => "\\\"",
        '\'' => "\\'",
        '\\' => "\\\\",
        c if c.is_control() => return s.push_str(&format!("\\u{:04x}", c as u32)),
        c => return s.push(c),
    };
    s.push_str(replace);
}
