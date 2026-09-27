//! Copies a callee's instructions into a caller under fresh local numbers.

use crate::Vm;
use nir::{Inst, Local, Next, Param, Val, ValId};
use rustc_hash::FxHashMap;

/// Gives a callee's locals fresh numbers in the caller.
pub struct Renamer<'a> {
    pub locals: FxHashMap<u64, Local>,
    pub next: &'a mut u64,
    pub vm: &'a mut Vm,
}

impl Renamer<'_> {
    pub fn fresh(&mut self) -> Local {
        *self.next += 1;
        Local(*self.next)
    }

    pub fn local(&mut self, l: Local) -> Local {
        if let Some(&new) = self.locals.get(&l.0) {
            return new;
        }
        let new = self.fresh();
        self.locals.insert(l.0, new);
        new
    }

    fn val(&mut self, v: ValId) -> ValId {
        let renamed = match self.vm.world.pool.val(v).clone() {
            Val::Local(l, ty) => Val::Local(self.local(l), ty),
            Val::StructValue(vs) => Val::StructValue(vs.iter().map(|v| self.val(*v)).collect()),
            Val::ArrayValue(ty, vs) => Val::ArrayValue(ty, vs.iter().map(|v| self.val(*v)).collect()),
            Val::Const(inner) => Val::Const(self.val(inner)),
            _ => return v,
        };
        ValId(self.vm.world.pool.vals.intern(renamed))
    }

    fn next(&mut self, n: &Next) -> Next {
        match n {
            Next::None => Next::None,
            Next::Unwind(p, to) => {
                Next::Unwind(Param { id: self.local(p.id), ty: p.ty }, Box::new(self.next(to)))
            }
            Next::Case(v, to) => Next::Case(self.val(*v), Box::new(self.next(to))),
            Next::Label(l, args) => Next::Label(self.local(*l), args.iter().map(|a| self.val(*a)).collect()),
        }
    }

    /// Where an instruction of the callee throws to: its own handler, else the call's.
    fn unwind(&mut self, n: &Next, call: &Next) -> Next {
        match n {
            Next::None => call.clone(),
            n => self.next(n),
        }
    }

    /// Callee instruction `inst` in the caller; returns continue at block `cont`.
    pub fn inst(&mut self, inst: &Inst, call_unwind: &Next, cont: Local) -> Inst {
        match inst {
            Inst::Label(l, params) => Inst::Label(
                self.local(*l),
                params.iter().map(|p| Param { id: self.local(p.id), ty: p.ty }).collect(),
            ),
            Inst::Let { id, op, unwind, scope } => Inst::Let {
                id: self.local(*id),
                op: op.map_operands(&mut |v| self.val(v)),
                unwind: self.unwind(unwind, call_unwind),
                scope: *scope,
            },
            Inst::Ret(v) => Inst::Jump(Next::Label(cont, vec![self.val(*v)])),
            Inst::Jump(n) => Inst::Jump(self.next(n)),
            Inst::If(c, t, e) => Inst::If(self.val(*c), self.next(t), self.next(e)),
            Inst::Switch(v, d, cases) => {
                Inst::Switch(self.val(*v), self.next(d), cases.iter().map(|c| self.next(c)).collect())
            }
            Inst::Throw(v, n) => Inst::Throw(self.val(*v), self.unwind(n, call_unwind)),
            Inst::Unreachable(n) => Inst::Unreachable(self.unwind(n, call_unwind)),
            Inst::LinktimeIf(..) => unreachable!("not inlined"),
        }
    }
}
