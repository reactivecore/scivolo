use super::code::{Arg, Code, Instr, Target};
use super::{Flow, Unwind};
use crate::{Vm, VmError};
use nir::Inst;

/// Arguments of one call, kept on the stack when there are few.
const INLINE_ARGS: usize = 12;

impl Vm {
    pub fn exec(&mut self, code: &Code, args: &[u64]) -> Flow {
        let mut slots = self.funcs.frames.pop().unwrap_or_default();
        slots.clear();
        slots.resize(code.slots, 0);
        for (&p, &a) in code.params.iter().zip(args) {
            slots[p as usize] = a;
        }
        let result = self.run(code, &mut slots);
        self.funcs.frames.push(slots);
        result
    }

    fn run(&mut self, code: &Code, slots: &mut [u64]) -> Flow {
        let mut pc = 0;
        loop {
            let instr = &code.instrs[pc];
            let next: &Target = match instr {
                Instr::Jump(t) => t,
                Instr::If { cond, then, other } => match self.arg(code, slots, *cond)? & 1 {
                    1 => then,
                    _ => other,
                },
                Instr::Switch { v, cases, default } => {
                    let v = self.arg(code, slots, *v)?;
                    cases.iter().find(|(k, _)| *k == v).map_or(default, |(_, t)| t)
                }
                Instr::LinktimeIf { inst, then, other } => {
                    let DefnBody(insts) = body(code);
                    let Inst::LinktimeIf(cond, ..) = &insts[*inst as usize] else {
                        unreachable!("lowered from it")
                    };
                    match self.linktime_cond(cond)? {
                        true => then,
                        false => other,
                    }
                }
                Instr::Ret(v) => return self.arg(code, slots, *v),
                Instr::Throw { v, unwind } => {
                    let exc = match self.arg(code, slots, *v)? as usize {
                        0 => match self.null_pointer::<u64>() {
                            Err(Unwind::Throw(npe)) => npe,
                            other => return other,
                        },
                        exc => exc,
                    };
                    match unwind {
                        Some(h) => {
                            slots[h.0 as usize] = exc as u64;
                            &h.1
                        }
                        None => return Err(Unwind::Throw(exc)),
                    }
                }
                Instr::Unreachable => return Err(VmError::Unsupported("reached unreachable".into()).into()),
                _ => match self.compute(code, slots, instr) {
                    Ok(()) => {
                        pc += 1;
                        continue;
                    }
                    Err(Unwind::Throw(exc)) => {
                        if self.funcs.trace {
                            eprintln!("  thrown by {instr:?}");
                        }
                        match instr.unwind() {
                            Some(h) => {
                                slots[h.0 as usize] = exc as u64;
                                &h.1
                            }
                            None => return Err(Unwind::Throw(exc)),
                        }
                    }
                    Err(fatal) => return Err(fatal),
                },
            };
            pc = self.jump(code, slots, next)?;
        }
    }

    /// Assigns the target block's parameters and returns its first instruction.
    fn jump(&mut self, code: &Code, slots: &mut [u64], t: &Target) -> Flow<usize> {
        match &*t.moves {
            [] => {}
            [(p, a)] => slots[*p as usize] = self.arg(code, slots, *a)?,
            moves => {
                let mut values = [0u64; INLINE_ARGS];
                let mut spill = Vec::new();
                for (i, (_, a)) in moves.iter().enumerate() {
                    let v = self.arg(code, slots, *a)?;
                    match i < INLINE_ARGS {
                        true => values[i] = v,
                        false => spill.push(v),
                    }
                }
                for (i, (p, _)) in moves.iter().enumerate() {
                    slots[*p as usize] = if i < INLINE_ARGS { values[i] } else { spill[i - INLINE_ARGS] };
                }
            }
        }
        Ok(t.pc as usize)
    }

    #[inline]
    pub fn arg(&mut self, code: &Code, slots: &[u64], a: Arg) -> Flow {
        match a {
            Arg::Slot(i) => Ok(slots[i as usize]),
            Arg::Imm(v) => Ok(v),
            Arg::Lazy(i) => {
                let (val, cell) = &code.lazy[i as usize];
                match cell.get() {
                    0 => {
                        let v = self.val(*val, &[])?;
                        cell.set(v);
                        Ok(v)
                    }
                    v => Ok(v),
                }
            }
        }
    }

    /// Calls `target` with `args` evaluated, without allocating for short argument lists.
    pub fn call_args(&mut self, code: &Code, slots: &[u64], target: u64, args: &[Arg]) -> Flow {
        if args.len() <= INLINE_ARGS {
            let mut values = [0u64; INLINE_ARGS];
            for (v, a) in values.iter_mut().zip(args) {
                *v = self.arg(code, slots, *a)?;
            }
            return self.call_value(target, &values[..args.len()]);
        }
        let values = args.iter().map(|a| self.arg(code, slots, *a)).collect::<Flow<Vec<_>>>()?;
        self.call_value(target, &values)
    }
}

pub struct DefnBody<'a>(pub &'a [Inst]);

pub fn body(code: &Code) -> DefnBody<'_> {
    match &code.defn.kind {
        nir::DefnKind::Define { body, .. } => DefnBody(&body.insts),
        _ => DefnBody(&[]),
    }
}
