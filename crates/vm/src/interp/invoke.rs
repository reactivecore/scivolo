//! Calls from lowered code: arguments go straight from the caller's slots into the callee's
//! frame.

use super::code::{Arg, Code};
use super::{Flow, Func, FuncId};
use crate::Vm;

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
        let mut frame = self.funcs.frames.pop().unwrap_or_default();
        frame.clear();
        frame.resize(callee.slots, 0);
        for (&p, &a) in callee.params.iter().zip(args) {
            match self.arg(caller, slots, a) {
                Ok(v) => frame[p as usize] = v,
                Err(e) => return Err(e),
            }
        }
        let result = self.run(&callee, &mut frame);
        self.funcs.recycle(frame);
        result
    }

    /// A virtual call of `sig` on `args[0]`, with the dispatch cached in `sites`.
    pub fn invoke(&mut self, caller: &Code, slots: &[u64], sig: nir::StrId, site: u32, args: &[Arg]) -> Flow {
        let obj = match self.arg(caller, slots, args[0])? as usize {
            0 => return self.null_pointer(),
            obj => obj,
        };
        let cls = self.class_of(obj) as u64 + 1;
        let (seen, target) = (&caller.sites[site as usize], &caller.sites[site as usize + 1]);
        if seen.get() != cls {
            target.set(self.virtual_method(obj, sig)? as u64);
            seen.set(cls);
        }
        self.call_direct(caller, slots, target.get() as FuncId, args)
    }

    /// Calls the function behind `target` with `args` evaluated in the caller's frame.
    pub fn call_args(&mut self, caller: &Code, slots: &[u64], target: u64, args: &[Arg]) -> Flow {
        self.call_evaluated(caller, slots, |vm, values| vm.call_value(target, values), args)
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
