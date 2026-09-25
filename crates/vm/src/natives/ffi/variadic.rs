//! Calls of variadic C functions (`printf` and friends).

use crate::interp::code::Num;

/// Up to 8 integer and 8 floating-point registers, then 16 words the callee finds on the stack.
type VarFn = extern "C" fn(
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
) -> u64;

const STACK_WORDS: usize = 16;

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
        if i >= named && cfg!(all(target_arch = "aarch64", target_vendor = "apple")) {
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
    let [a, b, c, d, e, f, g, h] = ints;
    let [p, q, r, s, t, u, w, x] = floats;
    let [s0, s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11, s12, s13, s14, s15] = stack;
    // SAFETY: `addr` is a variadic C function; its fixed arguments are in the registers the
    // C ABI assigns them, and its variadic arguments in the stack words that follow the sixteen
    // register parameters (Apple arm64) or in the next registers (elsewhere).
    let fun: VarFn = unsafe { std::mem::transmute(addr) };
    let out = fun(
        a, b, c, d, e, f, g, h, p, q, r, s, t, u, w, x, s0, s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11,
        s12, s13, s14, s15,
    );
    Some(match ret {
        Num::Bool | Num::I8 => out & 0xFF,
        Num::I16 | Num::U16 => out & 0xFFFF,
        Num::I32 => out & 0xFFFF_FFFF,
        _ => out,
    })
}
