//! Calls into C functions found with `dlsym`, for non-variadic signatures of up to eight
//! integer and eight floating-point arguments passed in registers.

pub mod variadic;

use crate::Vm;
use crate::interp::Flow;
use nir::{DefnKind, GlobalId, Type};
use std::ffi::CString;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Class {
    Int { bits: u32, signed: bool },
    Float,
    Double,
}

impl Class {
    /// `v`, a zero-extended value of this class, as the C ABI passes it in a register:
    /// narrow signed integers sign-extended.
    fn widen(self, v: u64) -> u64 {
        match self {
            Class::Int { bits: 8, signed: true } => v as i8 as i64 as u64,
            Class::Int { bits: 16, signed: true } => v as i16 as i64 as u64,
            _ => v,
        }
    }

    /// A register value returned by C, zero-extended from this class's width.
    fn narrow(self, v: u64) -> u64 {
        match self {
            Class::Int { bits, .. } if bits < 64 => v & ((1 << bits) - 1),
            _ => v,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Foreign {
    pub addr: usize,
    pub args: Vec<Class>,
    pub ret: Class,
}

fn class(ty: &Type) -> Option<Class> {
    match ty {
        Type::Float => Some(Class::Float),
        Type::Double => Some(Class::Double),
        Type::Vararg | Type::StructValue(_) | Type::ArrayValue(..) => None,
        Type::Bool => Some(Class::Int { bits: 8, signed: false }),
        Type::Byte => Some(Class::Int { bits: 8, signed: true }),
        Type::Char => Some(Class::Int { bits: 16, signed: false }),
        Type::Short => Some(Class::Int { bits: 16, signed: true }),
        Type::Int => Some(Class::Int { bits: 32, signed: true }),
        _ => Some(Class::Int { bits: 64, signed: false }),
    }
}

/// Makes the symbols of Scala Native's C runtime, and of the libraries listed in
/// `SCALAI_NATIVE_LIBS` (`:`-separated), visible to `dlsym`.
pub fn load_runtime() -> Result<(), String> {
    let extra = std::env::var("SCALAI_NATIVE_LIBS").unwrap_or_default();
    std::iter::once(snrt::PATH).chain(extra.split(':').filter(|p| !p.is_empty())).try_for_each(open)
}

fn open(lib: &str) -> Result<(), String> {
    let path = CString::new(lib).map_err(|e| e.to_string())?;
    // SAFETY: `path` is a valid C string; loading runs only the library's static initializers.
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
    match handle.is_null() {
        true => Err(format!("cannot load the native library {lib}")),
        false => Ok(()),
    }
}

/// The C function `name` declared by `g`, if the process can find and call it.
pub fn resolve(vm: &mut Vm, g: GlobalId, name: &str) -> Option<Foreign> {
    let defn = vm.world.defn(g).ok()?;
    let DefnKind::Declare { ty } = defn.kind else { return None };
    let Type::Function(args, ret) = vm.world.pool.ty(ty).clone() else { return None };
    let pool = &vm.world.pool;
    let args: Vec<Class> = args.iter().map(|t| class(pool.ty(*t))).collect::<Option<_>>()?;
    let ret = class(pool.ty(ret))?;
    let ints = args.iter().filter(|c| matches!(c, Class::Int { .. })).count();
    if ints > 8 || args.len() - ints > 8 {
        return None;
    }
    symbol(name).map(|addr| Foreign { addr, args, ret })
}

/// The address of C symbol `name` in the process; `errno` is left as it was.
pub fn symbol(name: &str) -> Option<usize> {
    let cname = CString::new(name).ok()?;
    let saved = errno();
    // SAFETY: `dlsym` with RTLD_DEFAULT and a valid C string only reads the symbol tables.
    let addr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, cname.as_ptr()) } as usize;
    set_errno(saved);
    (addr != 0).then_some(addr)
}

#[cfg(target_os = "macos")]
fn errno_location() -> *mut i32 {
    // SAFETY: `__error` returns the calling thread's errno cell and has no preconditions.
    unsafe { libc::__error() }
}

#[cfg(not(target_os = "macos"))]
fn errno_location() -> *mut i32 {
    // SAFETY: `__errno_location` returns the calling thread's errno cell and has no preconditions.
    unsafe { libc::__errno_location() }
}

fn errno() -> i32 {
    // SAFETY: the errno cell of the calling thread is always valid to read.
    unsafe { *errno_location() }
}

fn set_errno(v: i32) {
    // SAFETY: the errno cell of the calling thread is always valid to write.
    unsafe { *errno_location() = v }
}

type IntFn =
    extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64) -> u64;
type FloatFn =
    extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64) -> f64;

pub fn call(f: &Foreign, values: &[u64]) -> Flow {
    let (mut ints, mut floats) = ([0u64; 8], [0f64; 8]);
    let (mut ni, mut nf) = (0, 0);
    for (&c, &v) in f.args.iter().zip(values) {
        match c {
            Class::Int { .. } => {
                if crate::interp::is_func_ref(v) {
                    let what = "a function C cannot call (floating-point signature or no free slot)";
                    return Err(crate::VmError::Unsupported(format!("passing {what} to C")).into());
                }
                ints[ni] = c.widen(v);
                ni += 1;
            }
            Class::Float | Class::Double => {
                floats[nf] = f64::from_bits(v);
                nf += 1;
            }
        }
    }
    let [a, b, c, d, e, g, h, i] = ints;
    let [p, q, r, s, t, u, w, x] = floats;
    match f.ret {
        Class::Int { .. } => {
            // SAFETY: `addr` is a C function whose register arguments are all covered by the
            // sixteen passed here; extra registers are ignored by the callee.
            let fun: IntFn = unsafe { std::mem::transmute(f.addr) };
            Ok(f.ret.narrow(fun(a, b, c, d, e, g, h, i, p, q, r, s, t, u, w, x)))
        }
        Class::Float | Class::Double => {
            // SAFETY: as above, for a function returning in the floating-point register.
            let fun: FloatFn = unsafe { std::mem::transmute(f.addr) };
            let out = fun(a, b, c, d, e, g, h, i, p, q, r, s, t, u, w, x);
            Ok(match f.ret {
                Class::Float => (out.to_bits() as u32) as u64,
                _ => out.to_bits(),
            })
        }
    }
}
