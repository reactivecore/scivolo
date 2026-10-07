//! Classes the compiler defines itself or erases specially, and the erasure of constants.

use super::{Erased, OBJECT};

/// The erasure of a class by its qualified name, if it is special.
pub fn special(full: &str) -> Option<Erased> {
    Some(match full {
        "scala.Boolean" => Erased::Prim('z'),
        "scala.Char" => Erased::Prim('c'),
        "scala.Byte" => Erased::Prim('b'),
        "scala.Short" => Erased::Prim('s'),
        "scala.Int" => Erased::Prim('i'),
        "scala.Long" => Erased::Prim('j'),
        "scala.Float" => Erased::Prim('f'),
        "scala.Double" => Erased::Prim('d'),
        "scala.scalanative.runtime.RawPtr" => Erased::Prim('R'),
        "scala.scalanative.runtime.RawSize" => Erased::Prim('w'),
        "scala.Unit" => Erased::Unit,
        "scala.Nothing" => Erased::Nothing,
        "scala.Null" => Erased::Null,
        "scala.Any" | "scala.AnyVal" | "scala.AnyRef" | "scala.Matchable" | "scala.Singleton"
        | "scala.AnyKind" | "java.lang.Object" | "scala.caps.Pure" | "scala.*:" => {
            Erased::Class(OBJECT.into())
        }
        "scala.<repeated>" => Erased::Class("scala.collection.immutable.Seq".into()),
        "scala.Tuple" | "scala.NonEmptyTuple" => Erased::Class("scala.Product".into()),
        _ => return function(full),
    })
}

/// `FunctionN`, `ContextFunctionN` and their `Impure` forms: `FunctionN`, or `FunctionXXL` above
/// 22 parameters.
fn function(full: &str) -> Option<Erased> {
    let name = full.strip_prefix("scala.")?;
    let name = name.strip_prefix("Impure").unwrap_or(name);
    let name = name.strip_prefix("Context").unwrap_or(name);
    let arity: u32 = name.strip_prefix("Function")?.parse().ok()?;
    let class =
        if arity > 22 { "scala.runtime.FunctionXXL".into() } else { format!("scala.Function{arity}") };
    (class != full || arity > 22).then_some(Erased::Class(class))
}

/// The box class of the primitive of erased letter `c`.
pub fn boxed(c: char) -> Erased {
    let class = match c {
        'z' => "java.lang.Boolean",
        'c' => "java.lang.Character",
        'b' => "java.lang.Byte",
        's' => "java.lang.Short",
        'i' => "java.lang.Integer",
        'j' => "java.lang.Long",
        'f' => "java.lang.Float",
        'd' => "java.lang.Double",
        _ => OBJECT,
    };
    Erased::Class(class.into())
}

/// The erasure of a tuple of `arity` elements: `TupleN`, or `TupleXXL` above 22.
pub fn tuple(arity: usize) -> Erased {
    match arity {
        0 => Erased::Class("scala.Tuple$package$EmptyTuple$".into()),
        1..=22 => Erased::Class(format!("scala.Tuple{arity}")),
        _ => Erased::Class("scala.runtime.TupleXXL".into()),
    }
}

/// The classes a JVM array of any type bounded by them could not hold all values of.
pub const UNBOUNDED: [&str; 4] = ["scala.Any", "scala.AnyVal", "scala.Matchable", "scala.Singleton"];

pub fn constant(tag: u8) -> Erased {
    use crate::tags::{
        BYTEconst, CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LONGconst, NULLconst,
        SHORTconst, STRINGconst, TRUEconst, UNITconst,
    };
    match tag {
        TRUEconst | FALSEconst => Erased::Prim('z'),
        BYTEconst => Erased::Prim('b'),
        SHORTconst => Erased::Prim('s'),
        CHARconst => Erased::Prim('c'),
        INTconst => Erased::Prim('i'),
        LONGconst => Erased::Prim('j'),
        FLOATconst => Erased::Prim('f'),
        DOUBLEconst => Erased::Prim('d'),
        UNITconst => Erased::Unit,
        NULLconst => Erased::Null,
        STRINGconst => Erased::Class("java.lang.String".into()),
        _ => Erased::Class(OBJECT.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_and_tuples() {
        assert_eq!(special("scala.ImpureFunction1"), Some(Erased::Class("scala.Function1".into())));
        assert_eq!(special("scala.ContextFunction2"), Some(Erased::Class("scala.Function2".into())));
        assert_eq!(special("scala.Function23"), Some(Erased::Class("scala.runtime.FunctionXXL".into())));
        assert_eq!(special("scala.Function1"), None);
        assert_eq!(tuple(2), Erased::Class("scala.Tuple2".into()));
    }
}
