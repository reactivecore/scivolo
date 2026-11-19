//! The dynamic linker, signals and `errno` in portable mode: no C library is loaded, so no C
//! symbol exists; the externs of `table` stand in for the ones the runtime needs.

use crate::interp::code::Num;
use crate::{Vm, heap};

pub use super::table::extern_fn;

/// Loads nothing: portable mode has no C runtime.
pub fn load_runtime() -> Result<(), String> {
    Ok(())
}

/// Finds no C symbol.
pub fn symbol(_name: &str) -> Option<usize> {
    None
}

/// Names no C symbol.
pub fn symbol_at(_addr: usize) -> Option<String> {
    None
}

/// Returns the address of C variable `name`: of `environ`, an empty environment.
pub fn variable(vm: &mut Vm, name: &str) -> Option<usize> {
    (name == "environ").then(|| {
        let cell = vm.heap.permanent(16, heap::Blob::Words);
        heap::write(cell, (cell + 8) as u64);
        cell
    })
}

/// Watches nothing: the host stops an input by its own means.
pub fn watch_sigint() -> Result<(), String> {
    Ok(())
}

/// Returns 0: no signal arrives in portable mode.
pub fn take_sigint() -> u32 {
    0
}

/// Returns 0: no C function sets `errno` in portable mode.
pub fn errno() -> i32 {
    0
}

/// Ignores `v`: no C function reads `errno` in portable mode.
pub fn set_errno(_v: i32) {}

/// Ends the process with `code`, which on wasm32 traps.
pub fn exit_immediately(code: i32) -> ! {
    std::process::exit(code)
}

pub mod variadic {
    use super::Num;

    /// Calls nothing: no C function is found to call.
    pub fn call(_addr: usize, _named: usize, _args: &[(u64, Num)], _ret: Num) -> Option<u64> {
        None
    }
}
