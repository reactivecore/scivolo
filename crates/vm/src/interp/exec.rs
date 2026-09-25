use super::{Flow, Unwind, entry_params};
use crate::{Vm, VmError};
use nir::{Defn, DefnKind, Inst, Next};

const NO_LABEL: u32 = u32::MAX;

/// What a method body needs beyond its instructions: slot count and label positions.
pub struct Code {
    slots: usize,
    labels: Vec<u32>,
}

impl Code {
    pub fn new(defn: &Defn) -> Self {
        let DefnKind::Define { body, .. } = &defn.kind else { return Self { slots: 0, labels: Vec::new() } };
        let mut max = 0;
        let mut labels = Vec::new();
        for (i, inst) in body.insts.iter().enumerate() {
            match inst {
                Inst::Label(l, params) => {
                    let l = l.0 as usize;
                    if labels.len() <= l {
                        labels.resize(l + 1, NO_LABEL);
                    }
                    labels[l] = i as u32;
                    max = params.iter().fold(max.max(l), |m, p| m.max(p.id.0 as usize));
                }
                Inst::Let { id, unwind, .. } => max = max.max(id.0 as usize).max(unwind_slot(unwind)),
                Inst::Throw(_, unwind) | Inst::Unreachable(unwind) => max = max.max(unwind_slot(unwind)),
                _ => {}
            }
        }
        Self { slots: max + 1, labels }
    }
}

fn unwind_slot(n: &Next) -> usize {
    match n {
        Next::Unwind(p, _) => p.id.0 as usize,
        _ => 0,
    }
}

impl Vm {
    pub fn exec(&mut self, insts: &[Inst], code: &Code, args: &[u64]) -> Flow {
        let mut slots = vec![0u64; code.slots];
        for (p, &a) in entry_params(insts).iter().zip(args) {
            slots[p.id.0 as usize] = a;
        }
        let mut pc = 1;
        loop {
            let next = match &insts[pc] {
                Inst::Label(..) => {
                    pc += 1;
                    continue;
                }
                Inst::Let { id, op, unwind, .. } => match self.op(op, &slots) {
                    Ok(v) => {
                        slots[id.0 as usize] = v;
                        pc += 1;
                        continue;
                    }
                    Err(Unwind::Throw(exc)) => {
                        if self.funcs.trace {
                            eprintln!("  thrown by {}", nir::show::op(&self.world.pool, op));
                        }
                        self.handler(unwind, exc, &mut slots)?
                    }
                    Err(fatal) => return Err(fatal),
                },
                Inst::Ret(v) => return self.val(*v, &slots),
                Inst::Jump(next) => next,
                Inst::If(cond, then, other) => match self.val(*cond, &slots)? & 1 {
                    1 => then,
                    _ => other,
                },
                Inst::Switch(v, default, cases) => {
                    let v = self.val(*v, &slots)?;
                    self.switch(v, default, cases, &slots)?
                }
                Inst::Throw(v, unwind) => {
                    let exc = self.val(*v, &slots)? as usize;
                    let exc = match exc {
                        0 => match self.null_pointer::<u64>() {
                            Err(Unwind::Throw(npe)) => npe,
                            other => return other,
                        },
                        _ => exc,
                    };
                    self.handler(unwind, exc, &mut slots)?
                }
                Inst::Unreachable(_) => return Err(VmError::Unsupported("reached unreachable".into()).into()),
                Inst::LinktimeIf(cond, then, other) => match self.linktime_cond(cond)? {
                    true => then,
                    false => other,
                },
            };
            pc = self.goto(next, code, insts, &mut slots)?;
        }
    }

    fn handler<'n>(&mut self, unwind: &'n Next, exc: usize, slots: &mut [u64]) -> Flow<&'n Next> {
        match unwind {
            Next::Unwind(param, to) => {
                slots[param.id.0 as usize] = exc as u64;
                Ok(to)
            }
            _ => Err(Unwind::Throw(exc)),
        }
    }

    fn switch<'n>(&mut self, v: u64, default: &'n Next, cases: &'n [Next], slots: &[u64]) -> Flow<&'n Next> {
        for case in cases {
            if let Next::Case(value, to) = case
                && self.val(*value, slots)? == v
            {
                return Ok(to);
            }
        }
        Ok(default)
    }

    fn goto(&mut self, next: &Next, code: &Code, insts: &[Inst], slots: &mut [u64]) -> Flow<usize> {
        let Next::Label(label, args) = next else {
            return Err(VmError::Unsupported("jump without label".into()).into());
        };
        let at = code.labels[label.0 as usize] as usize;
        let Inst::Label(_, params) = &insts[at] else { unreachable!("labels index Label instructions") };
        if !args.is_empty() {
            let values = args.iter().map(|&a| self.val(a, slots)).collect::<Flow<Vec<_>>>()?;
            for (p, v) in params.iter().zip(values) {
                slots[p.id.0 as usize] = v;
            }
        }
        Ok(at + 1)
    }
}
