// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Constants.scala
//! Literal constants, printed as Java prints them (dotty's `Constant.stringValue`).

#[derive(Clone, PartialEq, Debug)]
pub enum Constant {
    Unit,
    Boolean(bool),
    Byte(i8),
    Short(i16),
    Char(u16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    /// A string in UTF-16 units.
    String(Box<[u16]>),
    Null,
}

impl Constant {
    /// The tag name dotty's `Constants` uses.
    pub fn tag(&self) -> &'static str {
        match self {
            Constant::Unit => "Unit",
            Constant::Boolean(_) => "Boolean",
            Constant::Byte(_) => "Byte",
            Constant::Short(_) => "Short",
            Constant::Char(_) => "Char",
            Constant::Int(_) => "Int",
            Constant::Long(_) => "Long",
            Constant::Float(_) => "Float",
            Constant::Double(_) => "Double",
            Constant::String(_) => "String",
            Constant::Null => "Null",
        }
    }

    /// The value as Java's `String.valueOf` writes it, in UTF-16 units.
    pub fn string_value(&self) -> Vec<u16> {
        let s = match self {
            Constant::Unit => "()".to_string(),
            Constant::Boolean(b) => b.to_string(),
            Constant::Byte(b) => b.to_string(),
            Constant::Short(s) => s.to_string(),
            Constant::Char(c) => return vec![*c],
            Constant::Int(i) => i.to_string(),
            Constant::Long(l) => l.to_string(),
            Constant::Float(f) => java_float(*f as f64, format!("{f:e}")),
            Constant::Double(d) => java_float(*d, format!("{d:e}")),
            Constant::String(s) => return s.to_vec(),
            Constant::Null => "null".to_string(),
        };
        s.encode_utf16().collect()
    }
}

/// Java's `Double.toString`/`Float.toString` of `x`, given its shortest digits as Rust's `{:e}`.
fn java_float(x: f64, sci: String) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let (mantissa, exp) = sci.split_once('e').expect("scientific notation");
    let exp: i32 = exp.parse().expect("an exponent");
    let (sign, mantissa) = mantissa.strip_prefix('-').map_or(("", mantissa), |m| ("-", m));
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    if (-3..7).contains(&exp) {
        let point = exp + 1;
        let (int, frac) = if point <= 0 {
            ("0".to_string(), format!("{}{digits}", "0".repeat(-point as usize)))
        } else if point as usize >= digits.len() {
            (format!("{digits}{}", "0".repeat(point as usize - digits.len())), String::new())
        } else {
            (digits[..point as usize].to_string(), digits[point as usize..].to_string())
        };
        let frac = if frac.is_empty() { "0".to_string() } else { frac };
        format!("{sign}{int}.{frac}")
    } else {
        let frac = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{sign}{}.{frac}E{exp}", &digits[..1])
    }
}

#[cfg(test)]
mod tests {
    use super::Constant;

    fn show(c: Constant) -> String {
        String::from_utf16(&c.string_value()).unwrap()
    }

    #[test]
    fn floats_print_as_in_java() {
        assert_eq!(show(Constant::Double(1.0)), "1.0");
        assert_eq!(show(Constant::Double(1e7)), "1.0E7");
        assert_eq!(show(Constant::Double(0.001)), "0.001");
        assert_eq!(show(Constant::Double(1.5e-4)), "1.5E-4");
        assert_eq!(show(Constant::Double(123.456)), "123.456");
        assert_eq!(show(Constant::Float(0.1)), "0.1");
        assert_eq!(show(Constant::Float(3.4028235e38)), "3.4028235E38");
        assert_eq!(show(Constant::Double(-2.5)), "-2.5");
    }
}
