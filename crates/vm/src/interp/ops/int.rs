//! `Int` and `Long` operations and comparisons, each resolved to one operation when lowered.

use super::arith;
use crate::interp::code::Num;
use nir::{Bin, Comp};

#[derive(Clone, Copy, Debug)]
pub enum IntBin {
    Add32,
    Sub32,
    Mul32,
    And32,
    Or32,
    Xor32,
    Shl32,
    Lshr32,
    Ashr32,
    /// Division and remainder by a constant other than 0 and -1.
    Div32,
    Rem32,
    Add64,
    Sub64,
    Mul64,
    And64,
    Or64,
    Xor64,
    Shl64,
    Lshr64,
    Ashr64,
    Div64,
    Rem64,
}

impl IntBin {
    /// The operation for `bin` on `num`, with `divisor` the right operand if it is a constant.
    pub fn of(bin: Bin, num: Num, divisor: Option<u64>) -> Option<IntBin> {
        use IntBin::*;
        let safe = |d: u64, bits: u32| d != 0 && d != u64::MAX >> (64 - bits);
        Some(match (num, bin) {
            (Num::I32, Bin::Iadd) => Add32,
            (Num::I32, Bin::Isub) => Sub32,
            (Num::I32, Bin::Imul) => Mul32,
            (Num::I32, Bin::And) => And32,
            (Num::I32, Bin::Or) => Or32,
            (Num::I32, Bin::Xor) => Xor32,
            (Num::I32, Bin::Shl) => Shl32,
            (Num::I32, Bin::Lshr) => Lshr32,
            (Num::I32, Bin::Ashr) => Ashr32,
            (Num::I32, Bin::Sdiv) if divisor.is_some_and(|d| safe(d, 32)) => Div32,
            (Num::I32, Bin::Srem) if divisor.is_some_and(|d| safe(d, 32)) => Rem32,
            (Num::I64, Bin::Iadd) => Add64,
            (Num::I64, Bin::Isub) => Sub64,
            (Num::I64, Bin::Imul) => Mul64,
            (Num::I64, Bin::And) => And64,
            (Num::I64, Bin::Or) => Or64,
            (Num::I64, Bin::Xor) => Xor64,
            (Num::I64, Bin::Shl) => Shl64,
            (Num::I64, Bin::Lshr) => Lshr64,
            (Num::I64, Bin::Ashr) => Ashr64,
            (Num::I64, Bin::Sdiv) if divisor.is_some_and(|d| safe(d, 64)) => Div64,
            (Num::I64, Bin::Srem) if divisor.is_some_and(|d| safe(d, 64)) => Rem64,
            _ => return None,
        })
    }

    /// `l op r` on zero-extended bit patterns.
    #[inline(always)]
    pub fn apply(self, l: u64, r: u64) -> u64 {
        use IntBin::*;
        let (a, b) = (l as u32, r as u32);
        match self {
            Add32 => a.wrapping_add(b) as u64,
            Sub32 => a.wrapping_sub(b) as u64,
            Mul32 => a.wrapping_mul(b) as u64,
            And32 => (a & b) as u64,
            Or32 => (a | b) as u64,
            Xor32 => (a ^ b) as u64,
            Shl32 => a.wrapping_shl(b) as u64,
            Lshr32 => a.wrapping_shr(b) as u64,
            Ashr32 => (a as i32).wrapping_shr(b) as u32 as u64,
            Div32 => ((a as i32) / (b as i32)) as u32 as u64,
            Rem32 => ((a as i32) % (b as i32)) as u32 as u64,
            Add64 => l.wrapping_add(r),
            Sub64 => l.wrapping_sub(r),
            Mul64 => l.wrapping_mul(r),
            And64 => l & r,
            Or64 => l | r,
            Xor64 => l ^ r,
            Shl64 => l.wrapping_shl(r as u32),
            Lshr64 => l.wrapping_shr(r as u32),
            Ashr64 => (l as i64).wrapping_shr(r as u32) as u64,
            Div64 => ((l as i64) / (r as i64)) as u64,
            Rem64 => ((l as i64) % (r as i64)) as u64,
        }
    }
}

/// A comparison: of `Int`s or `Long`s as one operation, else of any `Num`.
#[derive(Clone, Copy, Debug)]
pub enum Test {
    Eq32,
    Ne32,
    Slt32,
    Sle32,
    Sgt32,
    Sge32,
    Ult32,
    Ule32,
    Ugt32,
    Uge32,
    Eq64,
    Ne64,
    Slt64,
    Sle64,
    Sgt64,
    Sge64,
    Ult64,
    Ule64,
    Ugt64,
    Uge64,
    Other(Comp, Num),
}

impl Test {
    pub fn of(comp: Comp, num: Num) -> Test {
        use Test::*;
        match (num, comp) {
            (Num::I32, Comp::Ieq) => Eq32,
            (Num::I32, Comp::Ine) => Ne32,
            (Num::I32, Comp::Slt) => Slt32,
            (Num::I32, Comp::Sle) => Sle32,
            (Num::I32, Comp::Sgt) => Sgt32,
            (Num::I32, Comp::Sge) => Sge32,
            (Num::I32, Comp::Ult) => Ult32,
            (Num::I32, Comp::Ule) => Ule32,
            (Num::I32, Comp::Ugt) => Ugt32,
            (Num::I32, Comp::Uge) => Uge32,
            (Num::I64, Comp::Ieq) => Eq64,
            (Num::I64, Comp::Ine) => Ne64,
            (Num::I64, Comp::Slt) => Slt64,
            (Num::I64, Comp::Sle) => Sle64,
            (Num::I64, Comp::Sgt) => Sgt64,
            (Num::I64, Comp::Sge) => Sge64,
            (Num::I64, Comp::Ult) => Ult64,
            (Num::I64, Comp::Ule) => Ule64,
            (Num::I64, Comp::Ugt) => Ugt64,
            (Num::I64, Comp::Uge) => Uge64,
            _ => Other(comp, num),
        }
    }

    #[inline(always)]
    pub fn apply(self, l: u64, r: u64) -> bool {
        use Test::*;
        let (a, b) = (l as u32, r as u32);
        match self {
            Eq32 => a == b,
            Ne32 => a != b,
            Slt32 => (a as i32) < (b as i32),
            Sle32 => (a as i32) <= (b as i32),
            Sgt32 => (a as i32) > (b as i32),
            Sge32 => (a as i32) >= (b as i32),
            Ult32 => a < b,
            Ule32 => a <= b,
            Ugt32 => a > b,
            Uge32 => a >= b,
            Eq64 => l == r,
            Ne64 => l != r,
            Slt64 => (l as i64) < (r as i64),
            Sle64 => (l as i64) <= (r as i64),
            Sgt64 => (l as i64) > (r as i64),
            Sge64 => (l as i64) >= (r as i64),
            Ult64 => l < r,
            Ule64 => l <= r,
            Ugt64 => l > r,
            Uge64 => l >= r,
            Other(comp, num) => arith::comp(comp, num, l, r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interp::ops::arith::bin_total;

    #[test]
    fn tests_match_the_generic_comparison() {
        let values = [0u64, 1, 2, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF, u64::MAX, 1 << 63, 5];
        let comps = [
            Comp::Ieq,
            Comp::Ine,
            Comp::Slt,
            Comp::Sle,
            Comp::Sgt,
            Comp::Sge,
            Comp::Ult,
            Comp::Ule,
            Comp::Ugt,
            Comp::Uge,
        ];
        for num in [Num::I32, Num::I64] {
            let fit = |v: u64| if num == Num::I32 { v as u32 as u64 } else { v };
            for comp in comps {
                for &l in &values {
                    for &r in &values {
                        let (l, r) = (fit(l), fit(r));
                        assert_eq!(Test::of(comp, num).apply(l, r), arith::comp(comp, num, l, r), "{comp:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn matches_the_generic_operation() {
        let values =
            [0u64, 1, 2, 3, 7, 31, 32, 33, 63, 64, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF, u64::MAX, 1 << 63];
        let bins = [
            Bin::Iadd,
            Bin::Isub,
            Bin::Imul,
            Bin::And,
            Bin::Or,
            Bin::Xor,
            Bin::Shl,
            Bin::Lshr,
            Bin::Ashr,
            Bin::Sdiv,
            Bin::Srem,
        ];
        for num in [Num::I32, Num::I64] {
            let fit = |v: u64| if num == Num::I32 { v as u32 as u64 } else { v };
            for bin in bins {
                for &l in &values {
                    for &r in &values {
                        let (l, r) = (fit(l), fit(r));
                        let Some(op) = IntBin::of(bin, num, Some(r)) else { continue };
                        assert_eq!(Some(op.apply(l, r)), bin_total(bin, num, l, r), "{op:?} {l} {r}");
                    }
                }
            }
        }
    }
}
