//! Arithmetic with Scala Native's (JVM-compatible) semantics, on zero-extended bit patterns.

use crate::Vm;
use crate::interp::Flow;
use nir::{Bin, Comp, Conv, Type};

fn bits(ty: &Type) -> u32 {
    match ty {
        Type::Bool => 1,
        Type::Byte => 8,
        Type::Char | Type::Short => 16,
        Type::Int | Type::Float => 32,
        _ => 64,
    }
}

fn mask(v: u64, bits: u32) -> u64 {
    if bits >= 64 { v } else { v & ((1u64 << bits) - 1) }
}

fn signed(v: u64, bits: u32) -> i64 {
    if bits >= 64 { v as i64 } else { ((v << (64 - bits)) as i64) >> (64 - bits) }
}

fn float(ty: &Type, v: u64) -> f64 {
    match ty {
        Type::Float => f32::from_bits(v as u32) as f64,
        _ => f64::from_bits(v),
    }
}

fn from_float(ty: &Type, f: f64) -> u64 {
    match ty {
        Type::Float => (f as f32).to_bits() as u64,
        _ => f.to_bits(),
    }
}

impl Vm {
    pub fn bin(&mut self, bin: Bin, ty: &Type, l: u64, r: u64) -> Flow {
        let n = bits(ty);
        if matches!(ty, Type::Float | Type::Double) {
            let (a, b) = (float(ty, l), float(ty, r));
            let f = match ty {
                Type::Float => {
                    let (a, b) = (a as f32, b as f32);
                    (match bin {
                        Bin::Fadd => a + b,
                        Bin::Fsub => a - b,
                        Bin::Fmul => a * b,
                        Bin::Fdiv => a / b,
                        _ => a % b,
                    }) as f64
                }
                _ => match bin {
                    Bin::Fadd => a + b,
                    Bin::Fsub => a - b,
                    Bin::Fmul => a * b,
                    Bin::Fdiv => a / b,
                    _ => a % b,
                },
            };
            return Ok(from_float(ty, f));
        }
        let (sl, sr) = (signed(l, n), signed(r, n));
        let (ul, ur) = (mask(l, n), mask(r, n));
        let divides = matches!(bin, Bin::Sdiv | Bin::Srem | Bin::Udiv | Bin::Urem);
        if divides && ur == 0 {
            self.throw_runtime("D19throwDivisionByZeronEO", &[])?;
        }
        let shift = (r & (n as u64 - 1).max(1)) as u32;
        let v = match bin {
            Bin::Iadd => ul.wrapping_add(ur),
            Bin::Isub => ul.wrapping_sub(ur),
            Bin::Imul => ul.wrapping_mul(ur),
            Bin::Sdiv => signed_div(sl, sr, n, false),
            Bin::Srem => signed_div(sl, sr, n, true),
            Bin::Udiv => ul / ur,
            Bin::Urem => ul % ur,
            Bin::Shl => ul << shift,
            Bin::Lshr => ul >> shift,
            Bin::Ashr => (sl >> shift) as u64,
            Bin::And => ul & ur,
            Bin::Or => ul | ur,
            Bin::Xor => ul ^ ur,
            _ => from_float(ty, 0.0),
        };
        Ok(mask(v, n))
    }
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

pub fn comp(comp: Comp, ty: &Type, l: u64, r: u64) -> bool {
    let n = bits(ty);
    let (ul, ur, sl, sr) = (mask(l, n), mask(r, n), signed(l, n), signed(r, n));
    let (fl, fr) = (float(ty, l), float(ty, r));
    match comp {
        Comp::Ieq => ul == ur,
        Comp::Ine => ul != ur,
        Comp::Ugt => ul > ur,
        Comp::Uge => ul >= ur,
        Comp::Ult => ul < ur,
        Comp::Ule => ul <= ur,
        Comp::Sgt => sl > sr,
        Comp::Sge => sl >= sr,
        Comp::Slt => sl < sr,
        Comp::Sle => sl <= sr,
        Comp::Feq => fl == fr,
        Comp::Fne => fl != fr,
        Comp::Fgt => fl > fr,
        Comp::Fge => fl >= fr,
        Comp::Flt => fl < fr,
        Comp::Fle => fl <= fr,
    }
}

pub fn conv(conv: Conv, from: &Type, to: &Type, v: u64) -> u64 {
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
        Conv::Sitofp => from_float(to, signed(v, nf) as f64),
        Conv::Uitofp => from_float(to, mask(v, nf) as f64),
    };
    if matches!(conv, Conv::Sitofp | Conv::Uitofp) && matches!(to, Type::Float) {
        return sitofp_f32(conv, from, v);
    }
    mask(out, nt)
}

fn sitofp_f32(conv: Conv, from: &Type, v: u64) -> u64 {
    let n = bits(from);
    let f = match conv {
        Conv::Sitofp => signed(v, n) as f32,
        _ => mask(v, n) as f32,
    };
    f.to_bits() as u64
}

/// The module and mangled method that box or unbox values of box class `class`.
pub fn box_helper(class: &str, boxing: bool) -> Option<(&'static str, String)> {
    let (module, prim, short, unboxed) = match class {
        "java.lang.Boolean" => ("scala.runtime.BoxesRunTime$", "z", "Boolean", "Boolean"),
        "java.lang.Character" => ("scala.runtime.BoxesRunTime$", "c", "Character", "Char"),
        "java.lang.Byte" => ("scala.runtime.BoxesRunTime$", "b", "Byte", "Byte"),
        "java.lang.Short" => ("scala.runtime.BoxesRunTime$", "s", "Short", "Short"),
        "java.lang.Integer" => ("scala.runtime.BoxesRunTime$", "i", "Integer", "Int"),
        "java.lang.Long" => ("scala.runtime.BoxesRunTime$", "j", "Long", "Long"),
        "java.lang.Float" => ("scala.runtime.BoxesRunTime$", "f", "Float", "Float"),
        "java.lang.Double" => ("scala.runtime.BoxesRunTime$", "d", "Double", "Double"),
        other => {
            let short = other.rsplit('.').next()?;
            let prim = match short {
                "UByte" => "b",
                "UShort" => "s",
                "UInt" => "i",
                "ULong" => "j",
                "USize" | "Size" => "w",
                s if s == "Ptr" || s == "CArray" || s == "CVarArgList" || s.starts_with("CFuncPtr") => "R_",
                _ => return None,
            };
            ("scala.scalanative.runtime.Boxes$", prim, short, short)
        }
    };
    let (name, tys) = match boxing {
        true => (format!("boxTo{short}"), format!("{prim}L{}{class}", class.len())),
        false => (format!("unboxTo{unboxed}"), format!("L16java.lang.Object{prim}")),
    };
    Some((module, format!("D{}{name}{tys}EO", name.len())))
}
