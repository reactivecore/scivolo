//! Arithmetic with Scala Native's (JVM-compatible) semantics, on zero-extended bit patterns.

use crate::Vm;
use crate::interp::Flow;
use crate::interp::code::Num;
use nir::{Bin, Comp, Conv};

fn bits(n: Num) -> u32 {
    match n {
        Num::Bool => 1,
        Num::I8 => 8,
        Num::I16 | Num::U16 => 16,
        Num::I32 | Num::F32 => 32,
        Num::I64 | Num::F64 => 64,
    }
}

fn mask(v: u64, bits: u32) -> u64 {
    if bits >= 64 { v } else { v & ((1u64 << bits) - 1) }
}

fn signed(v: u64, bits: u32) -> i64 {
    if bits >= 64 { v as i64 } else { ((v << (64 - bits)) as i64) >> (64 - bits) }
}

fn float(n: Num, v: u64) -> f64 {
    match n {
        Num::F32 => f32::from_bits(v as u32) as f64,
        _ => f64::from_bits(v),
    }
}

fn from_float(n: Num, f: f64) -> u64 {
    match n {
        Num::F32 => (f as f32).to_bits() as u64,
        _ => f.to_bits(),
    }
}

fn float_bin(bin: Bin, num: Num, l: u64, r: u64) -> u64 {
    match num {
        Num::F32 => {
            let (a, b) = (f32::from_bits(l as u32), f32::from_bits(r as u32));
            let f = match bin {
                Bin::Fadd => a + b,
                Bin::Fsub => a - b,
                Bin::Fmul => a * b,
                Bin::Fdiv => a / b,
                _ => a % b,
            };
            f.to_bits() as u64
        }
        _ => {
            let (a, b) = (f64::from_bits(l), f64::from_bits(r));
            let f = match bin {
                Bin::Fadd => a + b,
                Bin::Fsub => a - b,
                Bin::Fmul => a * b,
                Bin::Fdiv => a / b,
                _ => a % b,
            };
            f.to_bits()
        }
    }
}

impl Vm {
    #[inline]
    pub fn bin(&mut self, bin: Bin, num: Num, l: u64, r: u64) -> Flow {
        match bin_total(bin, num, l, r) {
            Some(v) => Ok(v),
            None => self.throw_runtime("D19throwDivisionByZeronEO", &[]),
        }
    }
}

/// `l bin r`, or `None` for an integer division by zero.
#[inline(always)]
pub fn bin_total(bin: Bin, num: Num, l: u64, r: u64) -> Option<u64> {
    if matches!(num, Num::F32 | Num::F64) {
        return Some(float_bin(bin, num, l, r));
    }
    let n = bits(num);
    let (ul, ur) = (mask(l, n), mask(r, n));
    let v = match bin {
        Bin::Iadd => ul.wrapping_add(ur),
        Bin::Isub => ul.wrapping_sub(ur),
        Bin::Imul => ul.wrapping_mul(ur),
        Bin::And => ul & ur,
        Bin::Or => ul | ur,
        Bin::Xor => ul ^ ur,
        Bin::Shl => ul << (r & (n as u64 - 1).max(1)),
        Bin::Lshr => ul >> (r & (n as u64 - 1).max(1)),
        Bin::Ashr => (signed(l, n) >> (r & (n as u64 - 1).max(1))) as u64,
        Bin::Sdiv | Bin::Srem | Bin::Udiv | Bin::Urem if ur == 0 => return None,
        Bin::Sdiv => signed_div(signed(l, n), signed(r, n), n, false),
        Bin::Srem => signed_div(signed(l, n), signed(r, n), n, true),
        Bin::Udiv => ul / ur,
        Bin::Urem => ul % ur,
        _ => float_bin(bin, num, l, r),
    };
    Some(mask(v, n))
}

fn signed_div(a: i64, b: i64, bits: u32, rem: bool) -> u64 {
    let min = if bits >= 64 { i64::MIN } else { -(1i64 << (bits - 1)) };
    match (a == min && b == -1, rem) {
        (true, true) => 0,
        (true, false) => a as u64,
        (false, true) => (a % b) as u64,
        (false, false) => (a / b) as u64,
    }
}

#[inline(always)]
pub fn comp(comp: Comp, num: Num, l: u64, r: u64) -> bool {
    let n = bits(num);
    match comp {
        Comp::Ieq => mask(l, n) == mask(r, n),
        Comp::Ine => mask(l, n) != mask(r, n),
        Comp::Ugt => mask(l, n) > mask(r, n),
        Comp::Uge => mask(l, n) >= mask(r, n),
        Comp::Ult => mask(l, n) < mask(r, n),
        Comp::Ule => mask(l, n) <= mask(r, n),
        Comp::Sgt => signed(l, n) > signed(r, n),
        Comp::Sge => signed(l, n) >= signed(r, n),
        Comp::Slt => signed(l, n) < signed(r, n),
        Comp::Sle => signed(l, n) <= signed(r, n),
        Comp::Feq => float(num, l) == float(num, r),
        Comp::Fne => float(num, l) != float(num, r),
        Comp::Fgt => float(num, l) > float(num, r),
        Comp::Fge => float(num, l) >= float(num, r),
        Comp::Flt => float(num, l) < float(num, r),
        Comp::Fle => float(num, l) <= float(num, r),
    }
}

pub fn conv(conv: Conv, from: Num, to: Num, v: u64) -> u64 {
    let (nf, nt) = (bits(from), bits(to));
    let out = match conv {
        Conv::Trunc | Conv::Zext | Conv::Ptrtoint | Conv::Inttoptr | Conv::Bitcast | Conv::ZSizeCast => {
            mask(v, nf)
        }
        Conv::Sext | Conv::SSizeCast => signed(v, nf) as u64,
        Conv::Fptrunc | Conv::Fpext => from_float(to, float(from, v)),
        Conv::Fptosi => match nt {
            32 => float(from, v) as i32 as u32 as u64,
            _ => float(from, v) as i64 as u64,
        },
        Conv::Fptoui => match nt {
            32 => float(from, v) as u32 as u64,
            _ => float(from, v) as u64,
        },
        Conv::Sitofp | Conv::Uitofp => {
            let i = if conv == Conv::Sitofp { signed(v, nf) } else { mask(v, nf) as i64 };
            return match (to, conv) {
                (Num::F32, Conv::Sitofp) => (i as f32).to_bits() as u64,
                (Num::F32, _) => (i as u64 as f32).to_bits() as u64,
                (_, Conv::Sitofp) => (i as f64).to_bits(),
                _ => (i as u64 as f64).to_bits(),
            };
        }
    };
    mask(out, nt)
}
