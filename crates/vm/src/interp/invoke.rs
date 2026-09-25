//! Calls from lowered code: arguments go straight from the caller's slots into the callee's
//! frame.

use super::code::{Arg, Code};
use super::{Flow, Func, FuncId};
use crate::Vm;
use nir::{GlobalId, TypeId};
use std::cell::Cell;

/// Arguments of one call, kept on the stack when there are few.
const INLINE_ARGS: usize = 12;

impl Vm {
    /// Calls `f` with `args` evaluated in the caller's frame.
    pub fn call_direct(&mut self, caller: &Code, slots: &[u64], f: FuncId, args: &[Arg]) -> Flow {
        let callee = match &self.funcs.all[f as usize] {
            Func::Nir { code: Some(code), .. } if !self.funcs.instrumented() && code.shortcut.is_none() => {
                code.clone()
            }
            _ => return self.call_evaluated(caller, slots, |vm, values| vm.call(f, values), args),
        };
        if self.stack_exhausted() {
            return self.stack_overflow();
        }
        let base = self.frames.top();
        let frame = self.push_frame(callee.slots)?;
        for (&p, &a) in callee.params.iter().zip(args) {
            match self.arg(caller, slots, a) {
                Ok(v) => frame[p as usize] = v,
                Err(e) => {
                    self.frames.truncate(base);
                    return Err(e);
                }
            }
        }
        let result = self.run(&callee, frame);
        self.frames.truncate(base);
        result
    }

    /// A virtual call of `sig` on `args[0]`, with the dispatch cached at `site`.
    pub fn invoke(&mut self, caller: &Code, slots: &[u64], sig: nir::StrId, site: u32, args: &[Arg]) -> Flow {
        let receiver = self.arg(caller, slots, args[0])?;
        let f = self.cached_virtual(&caller.sites[site as usize..site as usize + 2], receiver, sig)?;
        self.call_direct(caller, slots, f, args)
    }

    /// The function `name`, cached in `cell` as its id plus one.
    pub fn cached_func(&mut self, cell: &Cell<u64>, name: GlobalId) -> Flow<FuncId> {
        if cell.get() == 0 {
            cell.set(self.func(name)? as u64 + 1);
        }
        Ok(cell.get() as FuncId - 1)
    }

    /// The implementation of `sig` for `receiver`'s class; `cells` hold the class id plus one
    /// and the function the last receiver class resolved to.
    pub fn cached_virtual(&mut self, cells: &[Cell<u64>], receiver: u64, sig: nir::StrId) -> Flow<FuncId> {
        if receiver == 0 {
            return self.null_pointer();
        }
        let cls = self.class_of(receiver as usize) as u64 + 1;
        if cells[0].get() != cls {
            cells[1].set(self.virtual_method(receiver as usize, sig)? as u64);
            cells[0].set(cls);
        }
        Ok(cells[1].get() as FuncId)
    }

    /// Calls the function behind `target`, a function value of function type `ty`, with `args`
    /// evaluated in the caller's frame.
    pub fn call_args(&mut self, caller: &Code, slots: &[u64], target: u64, ty: TypeId, args: &[Arg]) -> Flow {
        self.call_evaluated(caller, slots, |vm, values| vm.call_value_typed(target, ty, values), args)
    }

    fn call_evaluated(
        &mut self,
        caller: &Code,
        slots: &[u64],
        call: impl FnOnce(&mut Vm, &[u64]) -> Flow,
        args: &[Arg],
    ) -> Flow {
        if args.len() <= INLINE_ARGS {
            let mut values = [0u64; INLINE_ARGS];
            for (v, a) in values.iter_mut().zip(args) {
                *v = self.arg(caller, slots, *a)?;
            }
            return call(self, &values[..args.len()]);
        }
        let values = args.iter().map(|a| self.arg(caller, slots, *a)).collect::<Flow<Vec<_>>>()?;
        call(self, &values)
    }
}
