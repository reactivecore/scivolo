//! Calls of variadic C functions (`printf` and friends).

use crate::interp::code::Num;

const STACK_WORDS: usize = 32;

/// Up to 8 integer and 8 floating-point registers, then 32 words the callee finds on the stack.
#[rustfmt::skip]
type VarFn = extern "C" fn(
    u64, u64, u64, u64, u64, u64, u64, u64,
    f64, f64, f64, f64, f64, f64, f64, f64,
    u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
    u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
) -> u64;

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
/// `None` if the arguments do not fit the registers and stack words this path passes, or, off
/// Apple arm64, if a variadic argument is floating-point (x86-64 would need `%al` set).
pub fn call(addr: usize, named: usize, args: &[(u64, Num)], ret: Num) -> Option<u64> {
    let (mut ints, mut floats, mut stack) = ([0u64; 8], [0f64; 8], [0u64; STACK_WORDS]);
    let (mut ni, mut nf, mut ns) = (0, 0, 0);
    for (i, &(v, num)) in args.iter().enumerate() {
        let float = matches!(num, Num::F32 | Num::F64);
        if i >= named && cfg!(all(target_arch = "aarch64", target_vendor = "apple")) {
            *stack.get_mut(ns)? = promote(num, v);
            ns += 1;
        } else if float && i >= named {
            return None;
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
    let fun: VarFn = unsafe { std::mem::transmute(addr) };
    #[rustfmt::skip]
    let out = fun(
        a0, a1, a2, a3, a4, a5, a6, a7, f0, f1, f2, f3, f4, f5, f6, f7,
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10], s[11], s[12], s[13], s[14], s[15],
        s[16], s[17], s[18], s[19], s[20], s[21], s[22], s[23], s[24], s[25], s[26], s[27], s[28], s[29], s[30],
        s[31],
    );
    Some(match ret {
        Num::Bool | Num::I8 => out & 0xFF,
        Num::I16 | Num::U16 => out & 0xFFFF,
        Num::I32 => out & 0xFFFF_FFFF,
        _ => out,
    })
}
