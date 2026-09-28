//! Calls of variadic C functions (`printf` and friends).

use crate::interp::code::Num;

const STACK_WORDS: usize = 32;

const APPLE_ARM64: bool = cfg!(all(target_arch = "aarch64", target_vendor = "apple"));

/// The integer argument registers: 8 on arm64, 6 on x86-64.
const INT_REGS: usize = if APPLE_ARM64 { 8 } else { 6 };

/// Up to 8 integer and 8 floating-point registers, then 32 words the callee finds on the stack.
#[cfg(all(target_arch = "aarch64", target_vendor = "apple"))]
#[rustfmt::skip]
type VarFn = unsafe extern "C" fn(
    u64, u64, u64, u64, u64, u64, u64, u64,
    f64, f64, f64, f64, f64, f64, f64, f64,
    u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
    u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
) -> u64;

/// 6 integer and 8 floating-point registers, then the stack words, passed as C variadic
/// arguments, which sets `%al` on x86-64.
#[cfg(not(all(target_arch = "aarch64", target_vendor = "apple")))]
type VarFn = unsafe extern "C" fn(u64, ...) -> u64;

#[cfg(all(target_arch = "aarch64", target_vendor = "apple"))]
#[rustfmt::skip]
unsafe fn invoke(fun: VarFn, a: [u64; 8], f: [f64; 8], s: [u64; STACK_WORDS]) -> u64 {
    // SAFETY: the caller guarantees `fun` takes these registers and stack words.
    unsafe {
        fun(
            a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7],
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10], s[11], s[12], s[13], s[14], s[15],
            s[16], s[17], s[18], s[19], s[20], s[21], s[22], s[23], s[24], s[25], s[26], s[27], s[28], s[29], s[30],
            s[31],
        )
    }
}

#[cfg(not(all(target_arch = "aarch64", target_vendor = "apple")))]
#[rustfmt::skip]
unsafe fn invoke(fun: VarFn, a: [u64; 8], f: [f64; 8], s: [u64; STACK_WORDS]) -> u64 {
    // SAFETY: the caller guarantees `fun` takes these registers and stack words.
    unsafe {
        fun(
            a[0], a[1], a[2], a[3], a[4], a[5], f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7],
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10], s[11], s[12], s[13], s[14], s[15],
            s[16], s[17], s[18], s[19], s[20], s[21], s[22], s[23], s[24], s[25], s[26], s[27], s[28], s[29], s[30],
            s[31],
        )
    }
}

/// The bits a variadic argument of kind `num` is passed as: C promotes `float` to `double`
/// and small integers to `int`.
fn promote(num: Num, v: u64) -> u64 {
    match num {
        Num::F32 => (f32::from_bits(v as u32) as f64).to_bits(),
        Num::I8 => v as i8 as i64 as u64,
        Num::I16 => v as i16 as i64 as u64,
        Num::I32 => v as i32 as i64 as u64,
        _ => v,
    }
}

/// Calls the C function at `addr` whose first `named` parameters are fixed, with `args`;
/// `None` if the arguments do not fit the registers and stack words this path passes.
pub fn call(addr: usize, named: usize, args: &[(u64, Num)], ret: Num) -> Option<u64> {
    let (mut ints, mut floats, mut stack) = ([0u64; 8], [0f64; 8], [0u64; STACK_WORDS]);
    let (mut ni, mut nf, mut ns) = (0, 0, 0);
    for (i, &(v, num)) in args.iter().enumerate() {
        let float = matches!(num, Num::F32 | Num::F64);
        let spill = (i >= named && APPLE_ARM64) || if float { nf == 8 } else { ni == INT_REGS };
        if spill {
            *stack.get_mut(ns)? = promote(num, v);
            ns += 1;
        } else if float {
            floats[nf] = f64::from_bits(promote(num, v));
            nf += 1;
        } else {
            ints[ni] = promote(num, v);
            ni += 1;
        }
    }
    if matches!(ret, Num::F32 | Num::F64) {
        return None;
    }
    // SAFETY: `addr` is a variadic C function; its arguments are in the registers the C ABI
    // assigns them and the rest, in order, in the stack words after the register parameters
    // (on Apple arm64 all variadic arguments are on the stack).
    let out = unsafe { invoke(std::mem::transmute::<usize, VarFn>(addr), ints, floats, stack) };
    Some(match ret {
        Num::Bool | Num::I8 => out & 0xFF,
        Num::I16 | Num::U16 => out & 0xFFFF,
        Num::I32 => out & 0xFFFF_FFFF,
        _ => out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snprintf_with_ints_and_floats() {
        let mut buf = [0u8; 64];
        let fmt = c"%d %.2f %s %.1f %ld";
        let args = [
            (buf.as_mut_ptr() as u64, Num::I64),
            (buf.len() as u64, Num::I64),
            (fmt.as_ptr() as u64, Num::I64),
            (-7i32 as u32 as u64, Num::I32),
            (2.5f64.to_bits(), Num::F64),
            (c"x".as_ptr() as u64, Num::I64),
            (1.5f32.to_bits() as u64, Num::F32),
            (1u64 << 40, Num::I64),
        ];
        let n = call(super::super::symbol("snprintf").unwrap(), 3, &args, Num::I32).unwrap();
        let out = std::ffi::CStr::from_bytes_until_nul(&buf).unwrap();
        assert_eq!(out.to_str().unwrap(), "-7 2.50 x 1.5 1099511627776");
        assert_eq!(n as usize, out.count_bytes());
    }

    #[test]
    fn sprintf_with_more_arguments_than_registers() {
        let mut buf = [0u8; 128];
        let fmt = c"%d %d %d %d %d %d %d %d %d %d|%.0f %.0f %.0f %.0f %.0f %.0f %.0f %.0f %.0f %.0f";
        let mut args = vec![(buf.as_mut_ptr() as u64, Num::I64), (fmt.as_ptr() as u64, Num::I64)];
        args.extend((1..=10).map(|i| (i as u64, Num::I32)));
        args.extend((11..=20).map(|i| ((i as f64).to_bits(), Num::F64)));
        call(super::super::symbol("sprintf").unwrap(), 2, &args, Num::I32).unwrap();
        let out = std::ffi::CStr::from_bytes_until_nul(&buf).unwrap();
        assert_eq!(out.to_str().unwrap(), "1 2 3 4 5 6 7 8 9 10|11 12 13 14 15 16 17 18 19 20");
    }
}
