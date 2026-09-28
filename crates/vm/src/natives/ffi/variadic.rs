//! Calls of variadic C functions (`printf` and friends).

use crate::interp::code::Num;

const STACK_WORDS: usize = 32;

const APPLE_ARM64: bool = cfg!(all(target_arch = "aarch64", target_vendor = "apple"));

/// Up to 8 integer and 8 floating-point registers, then 32 words the callee finds on the stack.
#[cfg(all(target_arch = "aarch64", target_vendor = "apple"))]
#[rustfmt::skip]
type VarFn = unsafe extern "C" fn(
    u64, u64, u64, u64, u64, u64, u64, u64,
    f64, f64, f64, f64, f64, f64, f64, f64,
    u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
    u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
) -> u64;

/// The same arguments passed as C variadic arguments, which sets `%al` on x86-64.
#[cfg(not(all(target_arch = "aarch64", target_vendor = "apple")))]
type VarFn = unsafe extern "C" fn(u64, ...) -> u64;

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
        if i >= named && APPLE_ARM64 {
            *stack.get_mut(ns)? = promote(num, v);
            ns += 1;
        } else if float {
            *floats.get_mut(nf)? = f64::from_bits(promote(num, v));
            nf += 1;
        } else {
            *ints.get_mut(ni)? = promote(num, v);
            ni += 1;
        }
    }
    if matches!(ret, Num::F32 | Num::F64) {
        return None;
    }
    let [a0, a1, a2, a3, a4, a5, a6, a7] = ints;
    let [f0, f1, f2, f3, f4, f5, f6, f7] = floats;
    let s = stack;
    // SAFETY: `addr` is a variadic C function; its fixed arguments are in the registers the
    // C ABI assigns them, and its variadic arguments in the stack words that follow the sixteen
    // register parameters (Apple arm64) or in the next registers (elsewhere).
    #[rustfmt::skip]
    let out = unsafe {
        let fun: VarFn = std::mem::transmute(addr);
        fun(
            a0, a1, a2, a3, a4, a5, a6, a7, f0, f1, f2, f3, f4, f5, f6, f7,
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10], s[11], s[12], s[13], s[14], s[15],
            s[16], s[17], s[18], s[19], s[20], s[21], s[22], s[23], s[24], s[25], s[26], s[27], s[28], s[29], s[30],
            s[31],
        )
    };
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
}
