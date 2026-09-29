//! Methods simple enough to run without a frame: constants, getters and forwarders.

use super::code::{Arg, Code, Instr, Mem};
use super::{Flow, FuncId};
use crate::Vm;
use nir::{GlobalId, StrId};

const MAX_ARGS: usize = 8;

/// Where a forwarded argument comes from.
#[derive(Clone, Copy, Debug)]
pub enum Source {
    Param(usize),
    Imm(u64),
    Module,
}

#[derive(Debug)]
pub enum Shortcut {
    Const(u64),
    /// `this.field`, with the field offset cached at `site`.
    Field {
        mem: Mem,
        name: GlobalId,
        site: u32,
    },
    /// A call of another function with these arguments, at instruction `pc`; `module` is loaded
    /// first if any argument is `Source::Module`.
    Forward {
        callee: Callee,
        args: Box<[Source]>,
        module: Option<(GlobalId, u32)>,
        pc: u32,
    },
}

#[derive(Debug)]
pub enum Callee {
    Global { name: GlobalId, site: u32 },
    Virtual { sig: StrId, site: u32 },
}

/// The shortcut `instrs` with parameters `params` allow, if any.
pub fn detect(instrs: &[Instr], params: &[u32]) -> Option<Shortcut> {
    let param = |slot: u32| params.iter().position(|&p| p == slot);
    match instrs {
        [Instr::Ret(Arg::Imm(v)), Instr::Unreachable] => Some(Shortcut::Const(*v as u64)),
        [
            Instr::FieldLoad { dst, mem, obj: Arg::Slot(o), name, site, unwind: None },
            Instr::Ret(Arg::Slot(r)),
            Instr::Unreachable,
        ] if r == dst && param(*o) == Some(0) => {
            Some(Shortcut::Field { mem: *mem, name: *name, site: *site })
        }
        [body @ .., Instr::Ret(Arg::Slot(r)), Instr::Unreachable] => {
            let pc = body.iter().rposition(|i| !matches!(i, Instr::Nop))? as u32;
            let rest: Vec<&Instr> = body.iter().filter(|i| !matches!(i, Instr::Nop)).collect();
            let (module, call) = match rest[..] {
                [call] => (None, call),
                [Instr::Module { dst, name, site, unwind: None }, call] => (Some((*dst, *name, *site)), call),
                _ => return None,
            };
            let (dst, callee, args) = match call {
                Instr::CallGlobal { dst, name, site, args, unwind: None } => {
                    (dst, Callee::Global { name: *name, site: *site }, args)
                }
                Instr::Invoke { dst, sig, site, args, unwind: None } => {
                    (dst, Callee::Virtual { sig: *sig, site: *site }, args)
                }
                _ => return None,
            };
            if dst != r || args.len() > MAX_ARGS {
                return None;
            }
            let sources = args
                .iter()
                .map(|a| match *a {
                    Arg::Imm(v) => Some(Source::Imm(v as u64)),
                    Arg::Slot(s) if module.is_some_and(|(m, _, _)| m == s) => Some(Source::Module),
                    Arg::Slot(s) => param(s).map(Source::Param),
                    Arg::Lazy(_) => None,
                })
                .collect::<Option<Box<[Source]>>>()?;
            let module = module.map(|(_, name, site)| (name, site));
            Some(Shortcut::Forward { callee, args: sources, module, pc })
        }
        _ => None,
    }
}

impl Vm {
    /// Runs the shortcut of `code` with `args`, in a frame without slots unless it is a constant
    /// or a field read of a resolved field on an object.
    pub fn shortcut(&mut self, code: &Code, shortcut: &Shortcut, args: &[u64]) -> Flow {
        let pc = match shortcut {
            Shortcut::Const(v) => return Ok(*v),
            Shortcut::Field { mem, site, .. } => {
                let offset = code.sites[*site as usize].get();
                match args.first() {
                    Some(&obj) if obj != 0 && offset != 0 => {
                        return Ok(super::memory::load_mem(obj as usize + offset as usize - 1, *mem));
                    }
                    _ => 0,
                }
            }
            Shortcut::Forward { pc, .. } => *pc as usize,
        };
        let mark = self.frames.mark();
        if self.frames.push_frame(code.frame_method(), 0).is_none() {
            return self.stack_overflow();
        }
        self.frames.set_pc(pc);
        let result = self.run_shortcut(code, shortcut, args);
        self.frames.reset(mark);
        result
    }

    fn run_shortcut(&mut self, code: &Code, shortcut: &Shortcut, args: &[u64]) -> Flow {
        match shortcut {
            Shortcut::Const(v) => Ok(*v),
            Shortcut::Field { mem, name, site } => {
                let obj = match args.first() {
                    Some(&obj) if obj != 0 => obj as usize,
                    _ => return self.null_pointer(),
                };
                let cell = &code.sites[*site as usize];
                if cell.get() == 0 {
                    cell.set(self.field_offset(*name)? as u64 + 1);
                }
                Ok(super::memory::load_mem(obj + cell.get() as usize - 1, *mem))
            }
            Shortcut::Forward { callee, args: sources, module, .. } => {
                let module = match module {
                    Some((name, site)) => {
                        let cell = &code.sites[*site as usize];
                        match cell.get() {
                            0 => self.cached_module(cell, *name)?,
                            obj => obj,
                        }
                    }
                    None => 0,
                };
                let mut values = [0u64; MAX_ARGS];
                for (v, s) in values.iter_mut().zip(sources.iter()) {
                    *v = match *s {
                        Source::Param(i) => args.get(i).copied().unwrap_or(0),
                        Source::Imm(v) => v,
                        Source::Module => module,
                    };
                }
                let values = &values[..sources.len()];
                let f = self.forward_target(code, callee, values)?;
                self.call(f, values)
            }
        }
    }

    fn forward_target(&mut self, code: &Code, callee: &Callee, values: &[u64]) -> Flow<FuncId> {
        match *callee {
            Callee::Global { name, site } => self.cached_func(&code.sites[site as usize], name),
            Callee::Virtual { sig, site } => {
                let receiver = values.first().copied().unwrap_or(0);
                self.cached_virtual(&code.sites[site as usize..site as usize + 2], receiver, sig)
            }
        }
    }
}
