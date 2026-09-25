//! Methods simple enough to run without a frame: constants, getters and forwarders.

use super::code::{Arg, Code, Instr};
use super::{Flow, FuncId};
use crate::Vm;
use nir::{GlobalId, StrId, TypeId};

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
        ty: TypeId,
        name: GlobalId,
        site: u32,
    },
    /// A call of another function with these arguments; `module` is loaded first if any
    /// argument is `Source::Module`.
    Forward {
        callee: Callee,
        args: Box<[Source]>,
        module: Option<(GlobalId, u32)>,
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
        [Instr::Ret(Arg::Imm(v)), Instr::Unreachable] => Some(Shortcut::Const(*v)),
        [
            Instr::FieldLoad { dst, ty, obj: Arg::Slot(o), name, site, unwind: None },
            Instr::Ret(Arg::Slot(r)),
            Instr::Unreachable,
        ] if r == dst && param(*o) == Some(0) => Some(Shortcut::Field { ty: *ty, name: *name, site: *site }),
        [rest @ .., Instr::Ret(Arg::Slot(r)), Instr::Unreachable] => {
            let (module, call) = match rest {
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
                    Arg::Imm(v) => Some(Source::Imm(v)),
                    Arg::Slot(s) if module.is_some_and(|(m, _, _)| m == s) => Some(Source::Module),
                    Arg::Slot(s) => param(s).map(Source::Param),
                    Arg::Lazy(_) => None,
                })
                .collect::<Option<Box<[Source]>>>()?;
            let module = module.map(|(_, name, site)| (name, site));
            Some(Shortcut::Forward { callee, args: sources, module })
        }
        _ => None,
    }
}

impl Vm {
    /// Runs the shortcut of `code` with `args`.
    pub fn shortcut(&mut self, code: &Code, shortcut: &Shortcut, args: &[u64]) -> Flow {
        match shortcut {
            Shortcut::Const(v) => Ok(*v),
            Shortcut::Field { ty, name, site } => {
                let obj = match args.first() {
                    Some(&obj) if obj != 0 => obj as usize,
                    _ => return self.null_pointer(),
                };
                let cell = &code.sites[*site as usize];
                if cell.get() == 0 {
                    cell.set(self.field_offset(*name)? as u64 + 1);
                }
                Ok(self.load_ty(obj + cell.get() as usize - 1, *ty))
            }
            Shortcut::Forward { callee, args: sources, module } => {
                let module = match module {
                    Some((name, site)) => {
                        let cell = &code.sites[*site as usize];
                        if cell.get() == 0 {
                            cell.set(self.module(*name)? as u64);
                        }
                        cell.get()
                    }
                    None => 0,
                };
                let mut values = [0u64; MAX_ARGS];
                for (v, s) in values.iter_mut().zip(sources.iter()) {
                    *v = match *s {
                        Source::Param(i) => args[i],
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
            Callee::Global { name, site } => {
                let cell = &code.sites[site as usize];
                if cell.get() == 0 {
                    cell.set(self.func(name)? as u64 + 1);
                }
                Ok(cell.get() as FuncId - 1)
            }
            Callee::Virtual { sig, site } => {
                let obj = match values.first() {
                    Some(&obj) if obj != 0 => obj as usize,
                    _ => return self.null_pointer(),
                };
                let cls = self.class_of(obj) as u64 + 1;
                let (seen, target) = (&code.sites[site as usize], &code.sites[site as usize + 1]);
                if seen.get() != cls {
                    target.set(self.virtual_method(obj, sig)? as u64);
                    seen.set(cls);
                }
                Ok(target.get() as FuncId)
            }
        }
    }
}
