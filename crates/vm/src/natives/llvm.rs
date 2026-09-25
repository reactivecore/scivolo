//! LLVM intrinsics that library NIR calls as externs.

use super::NativeFn;
use crate::Vm;
use crate::heap;

fn f64_of(v: u64) -> f64 {
    f64::from_bits(v)
}

fn f32_of(v: u64) -> f32 {
    f32::from_bits(v as u32)
}

fn f64_bits(f: f64) -> u64 {
    f.to_bits()
}

fn f32_bits(f: f32) -> u64 {
    f.to_bits() as u64
}

macro_rules! unary {
    ($f:expr) => {
        |_, a| Ok($f(a[0]))
    };
}

macro_rules! binary {
    ($f:expr) => {
        |_, a| Ok($f(a[0], a[1]))
    };
}

pub fn intrinsic(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name.strip_prefix("llvm.")? {
        "ctlz.i8" => unary!(|v: u64| (v as u8).leading_zeros() as u64),
        "ctlz.i16" => unary!(|v: u64| (v as u16).leading_zeros() as u64),
        "ctlz.i32" => unary!(|v: u64| (v as u32).leading_zeros() as u64),
        "ctlz.i64" => unary!(|v: u64| v.leading_zeros() as u64),
        "cttz.i8" => unary!(|v: u64| (v as u8).trailing_zeros() as u64),
        "cttz.i16" => unary!(|v: u64| (v as u16).trailing_zeros() as u64),
        "cttz.i32" => unary!(|v: u64| (v as u32).trailing_zeros() as u64),
        "cttz.i64" => unary!(|v: u64| v.trailing_zeros() as u64),
        "ctpop.i16" => unary!(|v: u64| (v as u16).count_ones() as u64),
        "ctpop.i32" => unary!(|v: u64| (v as u32).count_ones() as u64),
        "ctpop.i64" => unary!(|v: u64| v.count_ones() as u64),
        "bswap.i16" => unary!(|v: u64| (v as u16).swap_bytes() as u64),
        "bswap.i32" => unary!(|v: u64| (v as u32).swap_bytes() as u64),
        "bswap.i64" => unary!(|v: u64| v.swap_bytes()),
        "bitreverse.i16" => unary!(|v: u64| (v as u16).reverse_bits() as u64),
        "bitreverse.i32" => unary!(|v: u64| (v as u32).reverse_bits() as u64),
        "bitreverse.i64" => unary!(|v: u64| v.reverse_bits()),
        "sqrt.f64" => unary!(|v| f64_bits(f64_of(v).sqrt())),
        "sin.f64" => unary!(|v| f64_bits(f64_of(v).sin())),
        "cos.f64" => unary!(|v| f64_bits(f64_of(v).cos())),
        "exp.f64" => unary!(|v| f64_bits(f64_of(v).exp())),
        "exp2.f64" => unary!(|v| f64_bits(f64_of(v).exp2())),
        "log.f64" => unary!(|v| f64_bits(f64_of(v).ln())),
        "log2.f64" => unary!(|v| f64_bits(f64_of(v).log2())),
        "log10.f64" => unary!(|v| f64_bits(f64_of(v).log10())),
        "fabs.f64" => unary!(|v| f64_bits(f64_of(v).abs())),
        "floor.f64" => unary!(|v| f64_bits(f64_of(v).floor())),
        "ceil.f64" => unary!(|v| f64_bits(f64_of(v).ceil())),
        "trunc.f64" => unary!(|v| f64_bits(f64_of(v).trunc())),
        "round.f64" => unary!(|v| f64_bits(f64_of(v).round())),
        "rint.f64" | "nearbyint.f64" => unary!(|v| f64_bits(f64_of(v).round_ties_even())),
        "pow.f64" => binary!(|a, b| f64_bits(f64_of(a).powf(f64_of(b)))),
        "powi.f64" => binary!(|a, b| f64_bits(f64_of(a).powi(b as i32))),
        "minnum.f64" => binary!(|a, b| f64_bits(f64_of(a).min(f64_of(b)))),
        "maxnum.f64" => binary!(|a, b| f64_bits(f64_of(a).max(f64_of(b)))),
        "copysign.f64" => binary!(|a, b| f64_bits(f64_of(a).copysign(f64_of(b)))),
        "fma.f64" => |_, a| Ok(f64_bits(f64_of(a[0]).mul_add(f64_of(a[1]), f64_of(a[2])))),
        "sqrt.f32" => unary!(|v| f32_bits(f32_of(v).sqrt())),
        "sin.f32" => unary!(|v| f32_bits(f32_of(v).sin())),
        "cos.f32" => unary!(|v| f32_bits(f32_of(v).cos())),
        "exp.f32" => unary!(|v| f32_bits(f32_of(v).exp())),
        "exp2.f32" => unary!(|v| f32_bits(f32_of(v).exp2())),
        "log.f32" => unary!(|v| f32_bits(f32_of(v).ln())),
        "log2.f32" => unary!(|v| f32_bits(f32_of(v).log2())),
        "log10.f32" => unary!(|v| f32_bits(f32_of(v).log10())),
        "fabs.f32" => unary!(|v| f32_bits(f32_of(v).abs())),
        "floor.f32" => unary!(|v| f32_bits(f32_of(v).floor())),
        "ceil.f32" => unary!(|v| f32_bits(f32_of(v).ceil())),
        "trunc.f32" => unary!(|v| f32_bits(f32_of(v).trunc())),
        "round.f32" => unary!(|v| f32_bits(f32_of(v).round())),
        "rint.f32" | "nearbyint.f32" => unary!(|v| f32_bits(f32_of(v).round_ties_even())),
        "pow.f32" => binary!(|a, b| f32_bits(f32_of(a).powf(f32_of(b)))),
        "powi.f32" => binary!(|a, b| f32_bits(f32_of(a).powi(b as i32))),
        "minnum.f32" => binary!(|a, b| f32_bits(f32_of(a).min(f32_of(b)))),
        "maxnum.f32" => binary!(|a, b| f32_bits(f32_of(a).max(f32_of(b)))),
        "copysign.f32" => binary!(|a, b| f32_bits(f32_of(a).copysign(f32_of(b)))),
        "fma.f32" => |_, a| Ok(f32_bits(f32_of(a[0]).mul_add(f32_of(a[1]), f32_of(a[2])))),
        "donothing" | "stacksave" | "stackrestore" => |_, _| Ok(0),
        "sadd.with.overflow.i32" => |vm, a| Ok(pair32(vm, (a[0] as i32).overflowing_add(a[1] as i32))),
        "ssub.with.overflow.i32" => |vm, a| Ok(pair32(vm, (a[0] as i32).overflowing_sub(a[1] as i32))),
        "smul.with.overflow.i32" => |vm, a| Ok(pair32(vm, (a[0] as i32).overflowing_mul(a[1] as i32))),
        "uadd.with.overflow.i32" => |vm, a| Ok(pair32(vm, overflow_u32(a, u32::overflowing_add))),
        "usub.with.overflow.i32" => |vm, a| Ok(pair32(vm, overflow_u32(a, u32::overflowing_sub))),
        "umul.with.overflow.i32" => |vm, a| Ok(pair32(vm, overflow_u32(a, u32::overflowing_mul))),
        "sadd.with.overflow.i64" => |vm, a| Ok(pair64(vm, (a[0] as i64).overflowing_add(a[1] as i64))),
        "ssub.with.overflow.i64" => |vm, a| Ok(pair64(vm, (a[0] as i64).overflowing_sub(a[1] as i64))),
        "smul.with.overflow.i64" => |vm, a| Ok(pair64(vm, (a[0] as i64).overflowing_mul(a[1] as i64))),
        "uadd.with.overflow.i64" => |vm, a| Ok(pair64(vm, overflow_u64(a, u64::overflowing_add))),
        "usub.with.overflow.i64" => |vm, a| Ok(pair64(vm, overflow_u64(a, u64::overflowing_sub))),
        "umul.with.overflow.i64" => |vm, a| Ok(pair64(vm, overflow_u64(a, u64::overflowing_mul))),
        _ => return None,
    };
    Some(f)
}

fn overflow_u32(a: &[u64], op: fn(u32, u32) -> (u32, bool)) -> (i32, bool) {
    let (r, o) = op(a[0] as u32, a[1] as u32);
    (r as i32, o)
}

fn overflow_u64(a: &[u64], op: fn(u64, u64) -> (u64, bool)) -> (i64, bool) {
    let (r, o) = op(a[0], a[1]);
    (r as i64, o)
}

/// Result structs live in a ring of reused cells: code extracts their fields right after the call.
const SCRATCH_CELLS: usize = 64;

fn scratch(vm: &mut Vm) -> usize {
    if vm.scratch.is_empty() {
        let base = vm.heap.alloc(16 * SCRATCH_CELLS);
        vm.scratch = (0..SCRATCH_CELLS).map(|i| base + 16 * i).collect();
    }
    vm.scratch.rotate_left(1);
    vm.scratch[0]
}

/// The struct `{i32, i1}` holding `(value, overflowed)`.
fn pair32(vm: &mut Vm, (value, overflowed): (i32, bool)) -> u64 {
    let s = scratch(vm);
    heap::write(s, value);
    heap::write(s + 4, overflowed as u8);
    s as u64
}

/// The struct `{i64, i1}` holding `(value, overflowed)`.
fn pair64(vm: &mut Vm, (value, overflowed): (i64, bool)) -> u64 {
    let s = scratch(vm);
    heap::write(s, value);
    heap::write(s + 8, overflowed as u8);
    s as u64
}
