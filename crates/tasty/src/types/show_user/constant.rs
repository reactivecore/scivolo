// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/printing/PlainPrinter.scala
//   compiler/src/dotty/tools/dotc/printing/ReplPrinter.scala
//! Constant types as their values print on the JVM: `1`, `true`, `()`, `1.0E10`.

use crate::view::{Const, constant};

pub fn text(tag: u8, bits: i64) -> String {
    match constant(tag, bits) {
        Some(Const::Unit) => "()".into(),
        Some(Const::Bool(b)) => b.to_string(),
        Some(Const::Null) => "null".into(),
        Some(Const::Byte(n)) => n.to_string(),
        Some(Const::Short(n)) => n.to_string(),
        Some(Const::Char(c)) => c.to_string(),
        Some(Const::Int(n)) => n.to_string(),
        Some(Const::Long(n)) => n.to_string(),
        Some(Const::Float(f)) => java(f as f64, format!("{f:?}"), format!("{f:e}")),
        Some(Const::Double(d)) => java(d, format!("{d:?}"), format!("{d:e}")),
        Some(Const::Str(_)) | None => format!("<{}>", crate::tags::name(tag)),
    }
}

/// `x` as Java's `toString` shows it, from its shortest decimal and scientific forms.
fn java(x: f64, decimal: String, scientific: String) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 || (1e-3..1e7).contains(&x.abs()) {
        return decimal;
    }
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    match mantissa.contains('.') {
        true => format!("{mantissa}E{exponent}"),
        false => format!("{mantissa}.0E{exponent}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_print_as_in_java() {
        let show = |d: f64| java(d, format!("{d:?}"), format!("{d:e}"));
        assert_eq!(show(1.0), "1.0");
        assert_eq!(show(0.5), "0.5");
        assert_eq!(show(1e7), "1.0E7");
        assert_eq!(show(1.5e-5), "1.5E-5");
        assert_eq!(show(-2.0), "-2.0");
    }
}
