//! Operands, block targets and control instructions of the lowering.

use super::finish::resolve;
use super::uses::local;
use super::{Lowerer, immediate, num};
use crate::interp::code::{Arg, Handler, Instr, Num, Target};
use nir::{Inst, Next, Op, TypeId, Val, ValId};
use std::cell::Cell;

impl Lowerer<'_> {
    pub fn slot(&mut self, local: u64) -> u32 {
        let next = self.slots.len() as u32;
        *self.slots.entry(local).or_insert(next)
    }

    pub fn site(&mut self) -> u32 {
        self.sites += 1;
        self.sites - 1
    }

    pub fn arg(&mut self, v: ValId) -> Arg {
        let v = resolve(self.pool, &self.aliases, v);
        match self.pool.val(v) {
            Val::Local(l, _) if self.constants.contains_key(&l.0) => Arg::Imm(self.constants[&l.0]),
            Val::Local(l, _) => Arg::Slot(self.slot(l.0)),
            other => match immediate(other) {
                Some(bits) => Arg::Imm(bits),
                None => {
                    let next = self.lazy.len() as u32;
                    let id = *self.lazy_ids.entry(v).or_insert(next);
                    if id == next {
                        self.lazy.push((v, Cell::new(0)));
                    }
                    Arg::Lazy(id)
                }
            },
        }
    }

    pub fn args(&mut self, vs: &[ValId]) -> Box<[Arg]> {
        vs.iter().map(|&v| self.arg(v)).collect()
    }

    pub fn val_num(&self, v: ValId) -> Num {
        super::val_num(self.pool, v)
    }

    pub fn num_of(&self, ty: TypeId) -> Num {
        num(self.pool.ty(ty))
    }

    /// Where `next` continues; a missing label continues at the trailing `Unreachable`.
    fn target(&mut self, next: &Next) -> Target {
        let nowhere = Target { pc: self.nowhere, moves: Box::default() };
        let Next::Label(label, args) = next else { return nowhere };
        let Some((pc, params)) = self.blocks.get(&label.0).cloned() else { return nowhere };
        let kept: Vec<(u32, ValId)> = params
            .into_iter()
            .zip(args)
            .filter(|((local, _), _)| !self.aliases.contains_key(local))
            .map(|((_, slot), &a)| (slot, a))
            .collect();
        let moves = kept.into_iter().map(|(slot, a)| (slot, self.arg(a))).collect();
        Target { pc, moves }
    }

    pub fn handler(&mut self, unwind: &Next) -> Handler {
        match unwind {
            Next::Unwind(param, to) => {
                let slot = self.slot(param.id.0);
                Some(Box::new((slot, self.target(to))))
            }
            _ => None,
        }
    }

    /// The comparison the `if` at NIR instruction `at` branches on, if lowered into it.
    fn branch_comp(&self, at: u32) -> Option<(nir::Comp, TypeId, ValId, ValId)> {
        match self.insts.get((at as usize).checked_sub(1)?)? {
            Inst::Let { id, op: Op::Comp { comp, ty, l, r }, .. } if self.branch_comps.contains(&id.0) => {
                Some((*comp, *ty, *l, *r))
            }
            _ => None,
        }
    }

    pub fn inst(&mut self, at: u32, inst: &Inst) -> Instr {
        match inst {
            Inst::Let { id, op: Op::Copy(_) | Op::Varload(_), .. } if self.aliases.contains_key(&id.0) => {
                Instr::Nop
            }
            Inst::Let { id, .. } if self.constants.contains_key(&id.0) => Instr::Nop,
            Inst::Let { id, op: Op::Comp { .. }, .. } if self.branch_comps.contains(&id.0) => Instr::Nop,
            Inst::Let { op: Op::Varstore { value, .. }, .. }
                if local(self.pool, *value).is_some_and(|v| self.writes.contains_key(&v)) =>
            {
                Instr::Nop
            }
            Inst::Let { id, op, unwind, .. } => {
                let dst = self.slot(self.writes.get(&id.0).copied().unwrap_or(id.0));
                self.op(id.0, dst, op, unwind)
            }
            Inst::Ret(v) => Instr::Ret(self.arg(*v)),
            Inst::Jump(next) => Instr::Jump(self.target(next)),
            Inst::If(cond, then, other) => match self.branch_comp(at) {
                Some((comp, ty, l, r)) => Instr::CompIf {
                    comp,
                    num: self.num_of(ty),
                    l: self.arg(l),
                    r: self.arg(r),
                    then: self.target(then),
                    other: self.target(other),
                },
                None => {
                    Instr::If { cond: self.arg(*cond), then: self.target(then), other: self.target(other) }
                }
            },
            Inst::Switch(v, default, cases) => {
                let cases = cases
                    .iter()
                    .filter_map(|c| match c {
                        Next::Case(value, to) => Some((self.arg(*value), self.target(to))),
                        _ => None,
                    })
                    .collect();
                Instr::Switch { v: self.arg(*v), cases, default: self.target(default) }
            }
            Inst::Throw(v, unwind) => Instr::Throw { v: self.arg(*v), unwind: self.handler(unwind) },
            Inst::LinktimeIf(_, then, other) => {
                Instr::LinktimeIf { inst: at, then: self.target(then), other: self.target(other) }
            }
            Inst::Unreachable(_) | Inst::Label(..) => Instr::Unreachable,
        }
    }
}
