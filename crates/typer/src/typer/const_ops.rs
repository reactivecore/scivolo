// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ConstFold.scala
//! Operations on constants as Java evaluates them (dotty's `ConstFold.foldUnop`, `foldBinop`).

use super::constants::{
    BOOLEAN, BYTE, DOUBLE, FLOAT, INT, LONG, NULL, STRING, double_value, float_value, int_value, is_numeric,
    long_value, tag,
};
use syntax::ast::constant::Constant as C;

/// Unary operation `op` on `x`, if it folds.
pub fn fold_unop(op: &str, x: &C) -> Option<C> {
    Some(match (op, x) {
        ("unary_!", C::Boolean(b)) => C::Boolean(!b),
        ("unary_~", C::Int(i)) => C::Int(!i),
        ("unary_~", C::Long(l)) => C::Long(!l),
        ("unary_+", C::Int(_) | C::Long(_) | C::Float(_) | C::Double(_)) => x.clone(),
        ("unary_-", C::Int(i)) => C::Int(i.wrapping_neg()),
        ("unary_-", C::Long(l)) => C::Long(l.wrapping_neg()),
        ("unary_-", C::Float(f)) => C::Float(-f),
        ("unary_-", C::Double(d)) => C::Double(-d),
        ("toChar", _) if is_numeric(x) => C::Char(int_value(x) as u16),
        ("toInt", _) if is_numeric(x) => C::Int(int_value(x)),
        ("toLong", _) if is_numeric(x) => C::Long(long_value(x)),
        ("toFloat", _) if is_numeric(x) => C::Float(float_value(x)),
        ("toDouble", _) if is_numeric(x) => C::Double(double_value(x)),
        _ => return None,
    })
}

fn compare<V: PartialOrd>(op: &str, a: V, b: V) -> Option<C> {
    Some(C::Boolean(match op {
        "==" => a == b,
        "!=" => a != b,
        "<" => a < b,
        ">" => a > b,
        "<=" => a <= b,
        ">=" => a >= b,
        _ => return None,
    }))
}

/// Binary operation `op` on `x` and `y`, if it folds.
pub fn fold_binop(op: &str, x: &C, y: &C) -> Option<C> {
    let optag = if tag(x) == tag(y) {
        tag(x)
    } else if is_numeric(x) && is_numeric(y) {
        tag(x).max(tag(y))
    } else if tag(x) == NULL || tag(y) == NULL {
        NULL
    } else {
        return None;
    };
    match optag {
        BOOLEAN => {
            let (&C::Boolean(a), &C::Boolean(b)) = (x, y) else { return None };
            Some(C::Boolean(match op {
                "||" | "|" => a | b,
                "^" => a ^ b,
                "&&" | "&" => a & b,
                "==" => a == b,
                "!=" => a != b,
                _ => return None,
            }))
        }
        BYTE..=INT => int_op(op, int_value(x), int_value(y)),
        LONG if tag(x) <= INT && matches!(op, "<<" | ">>>" | ">>") => {
            int_op(op, int_value(x), long_value(y) as i32)
        }
        LONG => {
            let (a, b) = (long_value(x), long_value(y));
            Some(C::Long(match op {
                "|" => a | b,
                "^" => a ^ b,
                "&" => a & b,
                "<<" => a.wrapping_shl(b as u32),
                ">>>" => (a as u64).wrapping_shr(b as u32) as i64,
                ">>" => a.wrapping_shr(b as u32),
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "*" => a.wrapping_mul(b),
                "/" if b != 0 => a.wrapping_div(b),
                "%" if b != 0 => a.wrapping_rem(b),
                "/" | "%" => return None,
                _ => return compare(op, a, b),
            }))
        }
        FLOAT => float_op(op, float_value(x), float_value(y), C::Float),
        DOUBLE => float_op(op, double_value(x), double_value(y), C::Double),
        STRING => {
            let (C::String(a), C::String(b)) = (x, y) else { return None };
            match op {
                "+" => Some(C::String([&a[..], &b[..]].concat().into())),
                "==" | "!=" => compare(op, a, b),
                _ => None,
            }
        }
        NULL => match op {
            "==" => Some(C::Boolean(tag(x) == tag(y))),
            "!=" => Some(C::Boolean(tag(x) != tag(y))),
            _ => None,
        },
        _ => None,
    }
}

fn int_op(op: &str, a: i32, b: i32) -> Option<C> {
    Some(C::Int(match op {
        "|" => a | b,
        "^" => a ^ b,
        "&" => a & b,
        "<<" => a.wrapping_shl(b as u32),
        ">>>" => (a as u32).wrapping_shr(b as u32) as i32,
        ">>" => a.wrapping_shr(b as u32),
        "+" => a.wrapping_add(b),
        "-" => a.wrapping_sub(b),
        "*" => a.wrapping_mul(b),
        "/" if b != 0 => a.wrapping_div(b),
        "%" if b != 0 => a.wrapping_rem(b),
        "/" | "%" => return None,
        _ => return compare(op, a, b),
    }))
}

fn float_op<V>(op: &str, a: V, b: V, make: fn(V) -> C) -> Option<C>
where
    V: PartialOrd + std::ops::Add<Output = V> + std::ops::Sub<Output = V>,
    V: std::ops::Mul<Output = V> + std::ops::Div<Output = V> + std::ops::Rem<Output = V>,
{
    Some(make(match op {
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => a / b,
        "%" => a % b,
        _ => return compare(op, a, b),
    }))
}
