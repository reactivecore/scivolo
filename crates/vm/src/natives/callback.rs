//! C-callable trampolines for interpreted functions passed to C as function pointers.
//! Only integer and pointer arguments (at most eight) are supported.

use crate::Vm;
use crate::interp::{FuncId, Unwind};
use std::cell::Cell;

const SLOTS: usize = 64;

thread_local! {
    static VM: Cell<*mut Vm> = const { Cell::new(std::ptr::null_mut()) };
    static TARGETS: [Cell<Option<(FuncId, usize)>>; SLOTS] = const { [const { Cell::new(None) }; SLOTS] };
}

/// Makes `vm` the target of trampoline calls on this thread while it runs.
pub fn enter(vm: &mut Vm) {
    VM.with(|c| c.set(vm));
}

fn dispatch(slot: usize, args: [u64; 8]) -> u64 {
    let Some((f, arity)) = TARGETS.with(|t| t[slot].get()) else { return 0 };
    let vm = VM.with(Cell::get);
    // SAFETY: `enter` stored the running VM; C only calls a trampoline while the VM is blocked
    // inside the foreign call that received it, so no other reference to the VM is in use.
    let vm = unsafe { &mut *vm };
    match vm.call(f, &args[..arity]) {
        Ok(v) => v,
        Err(Unwind::Throw(exc)) => {
            eprintln!("scalai: exception escaped a C callback: {}", vm.describe_throwable(exc));
            std::process::abort()
        }
        Err(Unwind::Fatal(e)) => {
            eprintln!("scalai: {e}");
            std::process::abort()
        }
    }
}

macro_rules! trampolines {
    ($($slot:literal)*) => {
        const TRAMPOLINES: [extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64; SLOTS] = [$({
            extern "C" fn t(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64) -> u64 {
                dispatch($slot, [a, b, c, d, e, f, g, h])
            }
            t
        }),*];
    };
}

trampolines!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
    32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63);

#[derive(Default)]
pub struct Callbacks {
    by_func: Vec<(FuncId, usize)>,
}

impl Callbacks {
    /// The C address that calls `f` with `arity` arguments, assigning a trampoline on first use.
    pub fn address(&mut self, f: FuncId, arity: usize) -> Option<usize> {
        let slot = match self.by_func.iter().position(|&(g, _)| g == f) {
            Some(slot) => slot,
            None if self.by_func.len() < SLOTS && arity <= 8 => {
                let slot = self.by_func.len();
                TARGETS.with(|t| t[slot].set(Some((f, arity))));
                self.by_func.push((f, arity));
                slot
            }
            None => return None,
        };
        Some(TRAMPOLINES[slot] as usize)
    }

    /// The function behind trampoline address `addr`, if it is one.
    pub fn target(&self, addr: usize) -> Option<FuncId> {
        TRAMPOLINES[..self.by_func.len()]
            .iter()
            .position(|&t| t as usize == addr)
            .map(|slot| self.by_func[slot].0)
    }
}
