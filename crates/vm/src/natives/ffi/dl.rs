//! The dynamic linker: loading the runtime's libraries, finding symbols, and `dlerror`.

use super::{errno, set_errno};
use crate::Vm;
use crate::interp::Flow;
use std::ffi::{CStr, CString};

/// Makes the symbols of Scala Native's C runtime, and of the libraries listed in
/// `SCIVOLO_NATIVE_LIBS` (`:`-separated), visible to `dlsym`.
pub fn load_runtime() -> Result<(), String> {
    let extra = std::env::var("SCIVOLO_NATIVE_LIBS").unwrap_or_default();
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

/// The address of C symbol `name` in the process, else of the runtime's `scivolo_<name>`
/// (`native/scivolo/libc_glue.c`); `errno` is left as it was and no `dlerror` message is left.
pub fn symbol(name: &str) -> Option<usize> {
    lookup(name).or_else(|| lookup(&format!("scivolo_{name}")))
}

fn lookup(name: &str) -> Option<usize> {
    let cname = CString::new(name).ok()?;
    let saved = errno();
    // SAFETY: `dlsym` with RTLD_DEFAULT and a valid C string only reads the symbol tables;
    // `dlerror` has no preconditions.
    let addr = unsafe {
        let addr = libc::dlsym(libc::RTLD_DEFAULT, cname.as_ptr()) as usize;
        libc::dlerror();
        addr
    };
    set_errno(saved);
    (addr != 0).then_some(addr)
}

/// The C functions the VM implements itself: `dlerror`, returning a copy of the message.
pub fn extern_fn(name: &str) -> Option<crate::natives::NativeFn> {
    match name {
        "dlerror" => Some(dlerror),
        _ => None,
    }
}

fn dlerror(vm: &mut Vm, _: &[u64]) -> Flow {
    // SAFETY: `dlerror` has no preconditions; a non-null result is a C string valid until the
    // next dl call, and is copied before one.
    let msg = unsafe {
        let p = libc::dlerror();
        (!p.is_null()).then(|| CStr::from_ptr(p).to_owned())
    };
    Ok(match msg {
        Some(msg) => {
            vm.dlerror = msg;
            vm.dlerror.as_ptr() as u64
        }
        None => 0,
    })
}
