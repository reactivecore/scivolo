//! C-callable trampolines for interpreted functions passed to C as function pointers.
//!
//! A C function pointer carries no context, so each trampoline finds its VM and function in a
//! per-thread slot table: the one piece of VM state outside `Vm` (ADR 0002). Slots are
//! assigned while a VM runs and released when its `run_main` returns. Only integer and
//! pointer parameters and results are supported, at most eight parameters.

use crate::Vm;
use crate::interp::code::Num;
use crate::interp::{FuncId, Unwind};
use std::cell::Cell;

const SLOTS: usize = 64;
const MAX_ARITY: usize = 8;

#[derive(Clone, Copy)]
struct Slot {
    vm: *mut Vm,
    f: FuncId,
    params: [Num; MAX_ARITY],
    arity: usize,
}

thread_local! {
    static TABLE: [Cell<Option<Slot>>; SLOTS] = const { [const { Cell::new(None) }; SLOTS] };
}

fn narrow(num: Num, v: u64) -> u64 {
    match num {
        Num::Bool | Num::I8 => v & 0xFF,
        Num::I16 | Num::U16 => v & 0xFFFF,
        Num::I32 | Num::F32 => v & 0xFFFF_FFFF,
        Num::I64 | Num::F64 => v,
    }
}

fn dispatch(slot: usize, args: [u64; MAX_ARITY]) -> u64 {
    let Some(s) = TABLE.with(|t| t[slot].get()) else {
        eprintln!("scalai: C called back into a VM that is no longer running");
        std::process::abort()
    };
    let mut values = [0u64; MAX_ARITY];
    for i in 0..s.arity {
        values[i] = narrow(s.params[i], args[i]);
    }
    // SAFETY: the slot was filled by the VM at `s.vm` while it ran and is cleared before its
    // `run_main` returns, so the VM is alive and pinned; C calls back only while the VM waits
    // inside the foreign call, so its own `&mut` borrow is not used until this call returns.
    let vm = unsafe { &mut *s.vm };
    match vm.call(s.f, &values[..s.arity]) {
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

/// The trampoline slots one VM owns.
#[derive(Default)]
pub struct Callbacks {
    owned: Vec<(usize, FuncId)>,
}

impl Callbacks {
    /// The C address that calls `f` of `vm`, assigning a trampoline on first use; `None` for
    /// floating-point signatures, too many parameters or no free slot.
    pub fn address(&mut self, vm: *mut Vm, f: FuncId, params: &[Num], ret: Num) -> Option<usize> {
        if let Some(&(slot, _)) = self.owned.iter().find(|&&(_, g)| g == f) {
            return Some(TRAMPOLINES[slot] as usize);
        }
        let floats = params.iter().chain([&ret]).any(|n| matches!(n, Num::F32 | Num::F64));
        if floats || params.len() > MAX_ARITY {
            return None;
        }
        let mut fixed = [Num::I64; MAX_ARITY];
        fixed[..params.len()].copy_from_slice(params);
        let slot = TABLE.with(|t| t.iter().position(|c| c.get().is_none()))?;
        TABLE.with(|t| t[slot].set(Some(Slot { vm, f, params: fixed, arity: params.len() })));
        self.owned.push((slot, f));
        Some(TRAMPOLINES[slot] as usize)
    }

    /// The function behind trampoline address `addr`, if this VM owns it.
    pub fn target(&self, addr: usize) -> Option<FuncId> {
        self.owned.iter().find(|&&(slot, _)| TRAMPOLINES[slot] as usize == addr).map(|&(_, f)| f)
    }

    /// Frees every slot; C calling one afterwards aborts with a message.
    pub fn release(&mut self) {
        for (slot, _) in self.owned.drain(..) {
            TABLE.with(|t| t[slot].set(None));
        }
    }
}
