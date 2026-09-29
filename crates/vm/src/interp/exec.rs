use super::code::{Arg, Code, Instr, Target};
use super::memory::{array_elem, load_mem};
use super::ops::arith;
use super::{Flow, Unwind};
use crate::{Vm, VmError};
use nir::Inst;

impl Vm {
    pub fn exec(&mut self, code: &Code, args: &[u64]) -> Flow {
        if self.heap.take_weak_refs_collected() {
            self.run_weak_handler()?;
        }
        self.tick();
        let mark = self.frames.mark();
        let slots = self.push_frame(code)?;
        for (i, &p) in code.params.iter().enumerate() {
            slots[p as usize] = args.get(i).copied().unwrap_or(0);
        }
        let result = self.run(code, slots);
        self.frames.reset(mark);
        result
    }

    /// The slots of a new frame for `code`, which the caller resets when its call ends.
    pub fn push_frame<'a>(&mut self, code: &Code) -> Flow<&'a mut [u64]> {
        match self.frames.push_frame(code.frame_method(), code.slots) {
            Some(at) => Ok(self.frames.slots(at, code.slots)),
            None => match self.stack_overflow() {
                Err(u) => Err(u),
                Ok(_) => unreachable!("stack_overflow throws"),
            },
        }
    }

    /// `bytes` zeroed bytes freed when the current call returns.
    pub fn frame_alloc(&mut self, bytes: usize) -> Flow {
        match self.frames.push(bytes) {
            Some(at) => Ok(at as u64),
            None => self.stack_overflow(),
        }
    }

    pub fn run(&mut self, code: &Code, slots: &mut [u64]) -> Flow {
        let mut pc = 0;
        let profile = self.funcs.profile.on;
        loop {
            let instr = &code.instrs[pc];
            if profile {
                self.funcs.profile.instr(instr);
            }
            let next: &Target = match instr {
                Instr::Int { op, dst, l, r } => {
                    let (l, r) = (self.arg(code, slots, *l)?, self.arg(code, slots, *r)?);
                    slots[*dst as usize] = op.apply(l, r);
                    pc += 1;
                    continue;
                }
                Instr::Bin { bin, num, dst, l, r, .. } => {
                    match self.binary(code, slots, pc, (*bin, *num), *l, *r) {
                        Ok(v) => {
                            slots[*dst as usize] = v;
                            pc += 1;
                            continue;
                        }
                        Err(u) => self.unwind_to(code, slots, pc, instr, u)?,
                    }
                }
                Instr::Comp { test, dst, l, r } => match self.pair(code, slots, *l, *r) {
                    Ok((l, r)) => {
                        slots[*dst as usize] = test.apply(l, r) as u64;
                        pc += 1;
                        continue;
                    }
                    Err(u) => self.unwind_to(code, slots, pc, instr, u)?,
                },
                Instr::Conv { conv, from, to, dst, v } => match self.arg(code, slots, *v) {
                    Ok(v) => {
                        slots[*dst as usize] = arith::conv(*conv, *from, *to, v);
                        pc += 1;
                        continue;
                    }
                    Err(u) => self.unwind_to(code, slots, pc, instr, u)?,
                },
                Instr::Copy { dst, v } => match self.arg(code, slots, *v) {
                    Ok(v) => {
                        slots[*dst as usize] = v;
                        pc += 1;
                        continue;
                    }
                    Err(u) => self.unwind_to(code, slots, pc, instr, u)?,
                },
                Instr::FieldLoad { dst, mem, obj: Arg::Slot(o), site, .. }
                    if slots[*o as usize] != 0 && code.sites[*site as usize].get() != 0 =>
                {
                    let at = slots[*o as usize] as usize + code.sites[*site as usize].get() as usize - 1;
                    slots[*dst as usize] = load_mem(at, *mem);
                    pc += 1;
                    continue;
                }
                Instr::ArrayLoad { dst, mem, arr: Arg::Slot(a), idx: Arg::Slot(i), .. } => {
                    match array_elem(slots[*a as usize], slots[*i as usize], mem.size()) {
                        Some(at) => {
                            slots[*dst as usize] = load_mem(at, *mem);
                            pc += 1;
                            continue;
                        }
                        None => match self.compute_at(code, slots, pc, instr)? {
                            Some(t) => t,
                            None => {
                                pc += 1;
                                continue;
                            }
                        },
                    }
                }
                Instr::Jump(t) => t,
                Instr::If { cond, then, other } => match self.arg(code, slots, *cond)? & 1 {
                    1 => then,
                    _ => other,
                },
                Instr::CompIf { test, l, r, then, other } => {
                    let (l, r) = (self.arg(code, slots, *l)?, self.arg(code, slots, *r)?);
                    match test.apply(l, r) {
                        true => then,
                        false => other,
                    }
                }
                Instr::Switch { v, cases, default } => {
                    let v = self.arg(code, slots, *v)?;
                    let mut next = default;
                    for (k, t) in cases.iter() {
                        if self.arg(code, slots, *k)? == v {
                            next = t;
                            break;
                        }
                    }
                    next
                }
                Instr::LinktimeIf { inst, then, other } => {
                    let DefnBody(insts) = body(code);
                    let Inst::LinktimeIf(cond, ..) = &insts[*inst as usize] else {
                        unreachable!("lowered from it")
                    };
                    match self.linktime_cond(cond, code.defn.name)? {
                        true => then,
                        false => other,
                    }
                }
                Instr::Ret(v) => return self.arg(code, slots, *v),
                Instr::Throw { v, unwind } => {
                    self.frames.set_pc(pc);
                    let value = self.arg(code, slots, *v)? as usize;
                    let exc = match self.throwable(value) {
                        Ok(exc) => exc,
                        Err(u) => match u.thrown() {
                            Some(exc) => exc,
                            None => return Err(u),
                        },
                    };
                    match unwind {
                        Some(h) => {
                            slots[h.0 as usize] = exc as u64;
                            &h.1
                        }
                        None => return Err(Unwind::throw(exc)),
                    }
                }
                Instr::Unreachable => return Err(VmError::Unsupported("reached unreachable".into()).into()),
                _ => match self.compute_at(code, slots, pc, instr)? {
                    Some(t) => t,
                    None => {
                        pc += 1;
                        continue;
                    }
                },
            };
            let target = self.jump(code, slots, next)?;
            if target <= pc {
                self.tick();
            }
            pc = target;
        }
    }
}

pub struct DefnBody<'a>(pub &'a [Inst]);

pub fn body(code: &Code) -> DefnBody<'_> {
    match &code.defn.kind {
        nir::DefnKind::Define { body, .. } => DefnBody(&body.insts),
        _ => DefnBody(&[]),
    }
}
