//! Calls into C functions found with `dlsym`, for non-variadic signatures of up to eight
//! integer and eight floating-point arguments passed in registers.

use crate::Vm;
use crate::interp::Flow;
use nir::{DefnKind, GlobalId, Type};
use std::ffi::CString;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Class {
    Int,
    Float,
    Double,
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
        _ => Some(Class::Int),
    }
}

/// Makes the symbols of Scala Native's C runtime visible to `dlsym`.
pub fn load_runtime() -> Result<(), String> {
    let path = CString::new(snrt::PATH).map_err(|e| e.to_string())?;
    // SAFETY: `path` is a valid C string; loading runs only the library's static initializers.
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
    match handle.is_null() {
        true => Err(format!("cannot load the C runtime at {}", snrt::PATH)),
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
    let ints = args.iter().filter(|c| **c == Class::Int).count();
    if ints > 8 || args.len() - ints > 8 {
        return None;
    }
    symbol(name).map(|addr| Foreign { addr, args, ret })
}

/// The address of C symbol `name` in the process.
pub fn symbol(name: &str) -> Option<usize> {
    let cname = CString::new(name).ok()?;
    // SAFETY: `dlsym` with RTLD_DEFAULT and a valid C string only reads the symbol tables.
    let addr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, cname.as_ptr()) } as usize;
    (addr != 0).then_some(addr)
}

type IntFn =
    extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64) -> u64;
type FloatFn =
    extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64) -> f64;

pub fn call(f: &Foreign, values: &[u64]) -> Flow {
    let (mut ints, mut floats) = ([0u64; 8], [0f64; 8]);
    let (mut ni, mut nf) = (0, 0);
    for (c, &v) in f.args.iter().zip(values) {
        match c {
            Class::Int => {
                ints[ni] = v;
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
        Class::Int => {
            // SAFETY: `addr` is a C function whose register arguments are all covered by the
            // sixteen passed here; extra registers are ignored by the callee.
            let fun: IntFn = unsafe { std::mem::transmute(f.addr) };
            Ok(fun(a, b, c, d, e, g, h, i, p, q, r, s, t, u, w, x))
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
