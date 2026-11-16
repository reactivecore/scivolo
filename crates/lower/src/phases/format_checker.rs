// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/localopt/FormatChecker.scala
//! The format check of `f` interpolations (dotty's `TypedFormatChecker.checked`): the format
//! string of the parts, `%s` inserted before a part without a leading conversion, every
//! conversion verified against its argument's type. Warnings are not reported.

use super::format_spec::{Checked, Conversion, Kind, conversions};
use super::interpolators::process_escapes;
use typer::tpd::Target;

/// The arguments of an interpolation as the format check asks about them.
pub trait Args {
    /// Whether argument `i` conforms to `t`.
    fn conforms(&mut self, i: usize, t: Target) -> bool;

    /// Whether an implicit view converts argument `i` to `t`, which then becomes the argument;
    /// fails if that is unknown.
    fn converts(&mut self, i: usize, t: Target) -> Checked<bool>;
}

/// The format string for interpolation parts `parts` and one argument fewer, `args`. Fails with
/// the reason if dotty reports an error.
pub fn checked(parts: &[&[u16]], args: &mut dyn Args) -> Checked<Vec<u16>> {
    let argc = parts.len() - 1;
    let mut format = Vec::new();
    for (n, part) in parts.iter().enumerate() {
        let part = process_escapes(part).ok_or("f interpolations with invalid escapes")?;
        let conversions = conversions(&part)?;
        for cv in &conversions {
            cv.verify(argc)?;
        }
        let mut rest = conversions.iter();
        let insert = |format: &mut Vec<u16>| format.extend("%s".encode_utf16());
        if n > 0 {
            match rest.next() {
                None => insert(&mut format),
                Some(cv) if cv.kind == Kind::Literal => insert(&mut format),
                Some(cv) if cv.is_indexed() && cv.index != u32::try_from(n).ok() => insert(&mut format),
                Some(cv) => {
                    if cv.start != 0 {
                        return Err("f interpolations with a conversion not following a splice");
                    }
                    let chosen = arg_type(n - 1, cv.variants(), args)?;
                    cv.accepts(chosen)?;
                }
            }
        }
        format.extend_from_slice(&part);
        for cv in rest {
            if n == 0 && cv.has('<') {
                return Err("f interpolations with `<` without a last argument");
            }
            if cv.kind != Kind::Literal && !cv.is_indexed() {
                return Err("f interpolations with a conversion not following a splice");
            }
        }
    }
    Ok(format)
}

/// The first of `variants` other than `Any` that argument `argi` conforms to, else the first a
/// view converts it to, else `Any` if among them (dotty's `argType`).
fn arg_type(argi: usize, variants: &[Target], args: &mut dyn Args) -> Checked<Target> {
    let classes = || variants.iter().copied().filter(|&t| t != Target::Any);
    if let Some(t) = classes().find(|&t| args.conforms(argi, t)) {
        return Ok(t);
    }
    for t in classes() {
        if args.converts(argi, t)? {
            return Ok(t);
        }
    }
    if variants.contains(&Target::Any) {
        return Ok(Target::Any);
    }
    Err("f interpolation arguments of a type the conversion does not take")
}

impl Conversion {
    /// Fails as dotty's `verify` reports an error.
    fn verify(&self, argc: usize) -> Checked<()> {
        let fail = |bad: bool, why| if bad { Err(why) } else { Ok(()) };
        let ok_flags = match self.kind {
            Kind::String => "-#<",
            Kind::Boolean | Kind::Hash => "-<",
            Kind::Literal => "-",
            _ => "-#+ 0,(<",
        };
        let goodies = || {
            fail(self.flags.chars().any(|f| !ok_flags.contains(f)), "f interpolations with an illegal flag")?;
            let in_range = self.index.is_none_or(|i| i > 0 && i as usize <= argc);
            fail(!in_range && !self.has('<'), "f interpolations with an argument index out of range")
        };
        let no_precision = || fail(self.precision.is_some(), "f interpolations with a precision not allowed");
        let only_minus = || {
            fail(self.flags.chars().any(|f| f != '-' && f != '<'), "f interpolations with a flag not allowed")
        };
        match self.kind {
            Kind::String | Kind::Boolean | Kind::Hash => goodies(),
            Kind::Character => {
                goodies()?;
                no_precision()?;
                only_minus()
            }
            Kind::Integral => {
                goodies()?;
                no_precision()?;
                fail(self.cc() == 'd' && self.has('#'), "f interpolations with `#` for `d`")?;
                fail(self.cc() != 'd' && self.has(','), "f interpolations with `,` for other than `d`")
            }
            Kind::FloatingPoint => {
                goodies()?;
                if matches!(self.cc(), 'a' | 'A') {
                    no_precision()?;
                    fail(self.has(',') || self.has('('), "f interpolations with a flag not allowed for `a`")?;
                }
                Ok(())
            }
            Kind::DateTime => {
                goodies()?;
                fail(self.op.len() != 2, "f interpolations with a one-character date/time conversion")?;
                fail(
                    !"HIklMSLNpzZsQBbhAaCYyjmdeRTrDFc".contains(self.cc()),
                    "f interpolations with an unknown date/time conversion",
                )?;
                no_precision()?;
                only_minus()
            }
            Kind::Literal if self.op == "%" => {
                goodies()?;
                no_precision()
            }
            Kind::Literal => fail(
                !self.flags.is_empty() || self.width.is_some() || self.precision.is_some(),
                "f interpolations with flags, width or precision for `%n`",
            ),
            Kind::Error => Err("f interpolations with an illegal conversion character"),
        }
    }

    /// The argument types the conversion takes (dotty's `acceptableVariants`).
    fn variants(&self) -> &'static [Target] {
        use Target::*;
        match self.kind {
            Kind::String if self.has('#') => &[Formattable],
            Kind::String | Kind::Hash => &[Any],
            Kind::Boolean => &[Boolean, Null, Any],
            Kind::Character => &[Char, Byte, Short, Int],
            Kind::Integral => &[Int, Long, Byte, Short, BigInt],
            Kind::FloatingPoint => &[Double, Float, BigDecimal],
            Kind::DateTime => &[Long, Calendar, Date],
            Kind::Literal | Kind::Error => &[],
        }
    }

    /// Fails as dotty's `accepts` reports an error for an argument of type `arg`.
    fn accepts(&self, arg: Target) -> Checked<()> {
        let signs = self.has('+') || self.has(' ') || self.has('(');
        if self.kind == Kind::Integral
            && arg != Target::BigInt
            && matches!(self.cc(), 'o' | 'x' | 'X')
            && signs
        {
            return Err("f interpolations with a sign flag for `o`, `x` or `X` of other than `BigInt`");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arguments of the targets they conform to, each with the one target a view converts it to.
    struct Typed<'a>(&'a [Target], &'a [Option<Target>]);

    impl Args for Typed<'_> {
        fn conforms(&mut self, i: usize, t: Target) -> bool {
            t == self.0[i]
        }

        fn converts(&mut self, i: usize, t: Target) -> Checked<bool> {
            Ok(self.1.get(i) == Some(&Some(t)))
        }
    }

    fn viewed(parts: &[&str], args: &[Target], views: &[Option<Target>]) -> Checked<String> {
        let parts: Vec<Vec<u16>> = parts.iter().map(|p| p.encode_utf16().collect()).collect();
        let parts: Vec<&[u16]> = parts.iter().map(Vec::as_slice).collect();
        let f = checked(&parts, &mut Typed(args, views))?;
        Ok(String::from_utf16(&f).unwrap())
    }

    fn format(parts: &[&str], args: &[Target]) -> Checked<String> {
        viewed(parts, args, &[])
    }

    #[test]
    fn inserts_default_conversions() {
        use Target::*;
        assert_eq!(
            format(&["a ", "%.2f b ", " c", "%n%%"], &[Double, Int, Int]).unwrap(),
            "a %.2f b %s c%s%n%%"
        );
        assert_eq!(format(&["x\\t", "%1$s%<s"], &[Int]).unwrap(), "x\t%1$s%<s");
        assert_eq!(
            format(&["", "%-5d|%05x"], &[Long]),
            Err("f interpolations with a conversion not following a splice")
        );
    }

    #[test]
    fn refuses_errors_and_conversions() {
        use Target::*;
        assert!(format(&["", "%d"], &[Double]).is_err());
        assert!(format(&["", "%#d"], &[Int]).is_err());
        assert!(format(&["", "%.2d"], &[Int]).is_err());
        assert!(format(&["", "%q"], &[Int]).is_err());
        assert!(format(&["100%"], &[]).is_err());
        assert!(format(&["", "%+x"], &[Int]).is_err());
        assert_eq!(format(&["", "%+x"], &[BigInt]).unwrap(), "%+x");
        assert_eq!(format(&["", "%tY"], &[Long]).unwrap(), "%tY");
    }

    #[test]
    fn takes_views_before_any() {
        use Target::*;
        assert_eq!(format(&["", "%b"], &[Int]).unwrap(), "%b");
        assert!(viewed(&["", "%c"], &[Any], &[Some(Char)]).is_ok());
        assert!(viewed(&["", "%c"], &[Any], &[Some(Boolean)]).is_err());
        assert!(viewed(&["", "%#s"], &[Any], &[Some(Formattable)]).is_ok());
    }
}
