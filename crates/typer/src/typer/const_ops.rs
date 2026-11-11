// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ConstFold.scala
//! Operations on constants as Java evaluates them (dotty's `ConstFold.foldUnop`, `foldBinop`).

use super::constants::{
    BOOLEAN, BYTE, DOUBLE, FLOAT, INT, LONG, NULL, STRING, double_value, float_value, int_value, is_numeric,
    long_value, tag,
};
use names::nme;
use syntax::ast::constant::Constant as C;

/// Unary operation `op` on `x`, if it folds.
pub fn fold_unop(op: &str, x: &C) -> Option<C> {
    Some(match (op, x) {
        (nme::UNARY_NOT, C::Boolean(b)) => C::Boolean(!b),
        (nme::UNARY_TILDE, C::Int(i)) => C::Int(!i),
        (nme::UNARY_TILDE, C::Long(l)) => C::Long(!l),
        (nme::UNARY_PLUS, C::Int(_) | C::Long(_) | C::Float(_) | C::Double(_)) => x.clone(),
        (nme::UNARY_MINUS, C::Int(i)) => C::Int(i.wrapping_neg()),
        (nme::UNARY_MINUS, C::Long(l)) => C::Long(l.wrapping_neg()),
        (nme::UNARY_MINUS, C::Float(f)) => C::Float(-f),
        (nme::UNARY_MINUS, C::Double(d)) => C::Double(-d),
        (nme::TO_CHAR, _) if is_numeric(x) => C::Char(int_value(x) as u16),
        (nme::TO_INT, _) if is_numeric(x) => C::Int(int_value(x)),
        (nme::TO_LONG, _) if is_numeric(x) => C::Long(long_value(x)),
        (nme::TO_FLOAT, _) if is_numeric(x) => C::Float(float_value(x)),
        (nme::TO_DOUBLE, _) if is_numeric(x) => C::Double(double_value(x)),
        _ => return None,
    })
}

fn compare<V: PartialOrd>(op: &str, a: V, b: V) -> Option<C> {
    Some(C::Boolean(match op {
        nme::EQ => a == b,
        nme::NE => a != b,
        nme::LT => a < b,
        nme::GT => a > b,
        nme::LE => a <= b,
        nme::GE => a >= b,
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
                nme::ZOR | nme::OR => a | b,
                nme::XOR => a ^ b,
                nme::ZAND | nme::AND => a & b,
                nme::EQ => a == b,
                nme::NE => a != b,
                _ => return None,
            }))
        }
        BYTE..=INT => int_op(op, int_value(x), int_value(y)),
        LONG if tag(x) <= INT && matches!(op, nme::LSL | nme::LSR | nme::ASR) => {
            int_op(op, int_value(x), long_value(y) as i32)
        }
        LONG => {
            let (a, b) = (long_value(x), long_value(y));
            Some(C::Long(match op {
                nme::OR => a | b,
                nme::XOR => a ^ b,
                nme::AND => a & b,
                nme::LSL => a.wrapping_shl(b as u32),
                nme::LSR => (a as u64).wrapping_shr(b as u32) as i64,
                nme::ASR => a.wrapping_shr(b as u32),
                nme::ADD => a.wrapping_add(b),
                nme::MINUS => a.wrapping_sub(b),
                nme::MUL => a.wrapping_mul(b),
                nme::DIV if b != 0 => a.wrapping_div(b),
                nme::MOD if b != 0 => a.wrapping_rem(b),
                nme::DIV | nme::MOD => return None,
                _ => return compare(op, a, b),
            }))
        }
        FLOAT => float_op(op, float_value(x), float_value(y), C::Float),
        DOUBLE => float_op(op, double_value(x), double_value(y), C::Double),
        STRING => {
            let (C::String(a), C::String(b)) = (x, y) else { return None };
            match op {
                nme::ADD => Some(C::String([&a[..], &b[..]].concat().into())),
                nme::EQ | nme::NE => compare(op, a, b),
                _ => None,
            }
        }
        NULL => match op {
            nme::EQ => Some(C::Boolean(tag(x) == tag(y))),
            nme::NE => Some(C::Boolean(tag(x) != tag(y))),
            _ => None,
        },
        _ => None,
    }
}

fn int_op(op: &str, a: i32, b: i32) -> Option<C> {
    Some(C::Int(match op {
        nme::OR => a | b,
        nme::XOR => a ^ b,
        nme::AND => a & b,
        nme::LSL => a.wrapping_shl(b as u32),
        nme::LSR => (a as u32).wrapping_shr(b as u32) as i32,
        nme::ASR => a.wrapping_shr(b as u32),
        nme::ADD => a.wrapping_add(b),
        nme::MINUS => a.wrapping_sub(b),
        nme::MUL => a.wrapping_mul(b),
        nme::DIV if b != 0 => a.wrapping_div(b),
        nme::MOD if b != 0 => a.wrapping_rem(b),
        nme::DIV | nme::MOD => return None,
        _ => return compare(op, a, b),
    }))
}

fn float_op<V>(op: &str, a: V, b: V, make: fn(V) -> C) -> Option<C>
where
    V: PartialOrd + std::ops::Add<Output = V> + std::ops::Sub<Output = V>,
    V: std::ops::Mul<Output = V> + std::ops::Div<Output = V> + std::ops::Rem<Output = V>,
{
    Some(make(match op {
        nme::ADD => a + b,
        nme::MINUS => a - b,
        nme::MUL => a * b,
        nme::DIV => a / b,
        nme::MOD => a % b,
        _ => return compare(op, a, b),
    }))
}
