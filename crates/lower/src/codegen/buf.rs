//! The instructions of a method body as nscplugin's `FixupBuffer` collects them: a label
//! before code that follows control flow, `unreachable` after a `Nothing` value.

use super::Gen;
use nir::{Inst, Local, Next, Param, PosId, Type, TypeId, Val, ValId};

#[derive(Default)]
pub struct Buf {
    pub insts: Vec<Inst>,
    pub positions: Vec<PosId>,
    labeled: bool,
}

fn is_control_flow(inst: &Inst) -> bool {
    !matches!(inst, Inst::Label(..) | Inst::Let { .. })
}

impl Gen<'_, '_> {
    /// Appends `inst` at the current position.
    pub fn emit_inst(&mut self, inst: Inst) {
        let pos = self.m.pos;
        self.push(inst, pos);
    }

    pub fn push(&mut self, inst: Inst, pos: PosId) {
        if let Inst::Label(..) = inst {
            if self.m.buf.labeled {
                let unwind = self.unwind();
                self.push(Inst::Unreachable(unwind), pos);
            }
            self.m.buf.labeled = true;
        } else {
            if !self.m.buf.labeled {
                let l = self.fresh();
                self.push(Inst::Label(l, Vec::new()), pos);
            }
            self.m.buf.labeled = !is_control_flow(&inst);
        }
        let nothing = match &inst {
            Inst::Let { op, .. } => self.resty_is_nothing(op),
            _ => false,
        };
        self.m.buf.insts.push(inst);
        self.m.buf.positions.push(pos);
        if nothing {
            let unwind = self.unwind();
            self.push(Inst::Unreachable(unwind), pos);
            let l = self.fresh();
            self.push(Inst::Label(l, Vec::new()), pos);
        }
    }

    pub fn label(&mut self, id: Local, params: Vec<Param>) {
        self.emit_inst(Inst::Label(id, params));
    }

    pub fn jump(&mut self, to: Local, args: Vec<ValId>) {
        self.emit_inst(Inst::Jump(Next::Label(to, args)));
    }

    pub fn ret(&mut self, v: ValId) {
        self.emit_inst(Inst::Ret(v));
    }

    pub fn branch(&mut self, cond: ValId, then: Local, else_: Local) {
        let (then, else_) = (Next::Label(then, Vec::new()), Next::Label(else_, Vec::new()));
        self.emit_inst(Inst::If(cond, then, else_));
    }

    pub fn raise(&mut self, v: ValId) {
        let unwind = self.unwind();
        self.emit_inst(Inst::Throw(v, unwind));
    }

    /// A jump to merge label `to`, passing `value` unless the merged type is `Unit`.
    pub fn jump_exclude_unit(&mut self, merge_ty: TypeId, to: Local, value: ValId) {
        let args = if *self.pool.ty(merge_ty) == Type::Unit { Vec::new() } else { vec![value] };
        self.jump(to, args);
    }

    /// Merge label `id` with parameter `value` of type `ty`, without it for `Unit`; the merged
    /// value.
    pub fn label_exclude_unit(&mut self, id: Local, value: Local, ty: TypeId) -> ValId {
        if *self.pool.ty(ty) == Type::Unit {
            self.label(id, Vec::new());
            self.val(Val::Unit)
        } else {
            self.label(id, vec![Param { id: value, ty }]);
            self.val(Val::Local(value, ty))
        }
    }

    /// Generates into a buffer of its own; returns its instructions and positions.
    pub fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> (T, Buf) {
        let outer = std::mem::take(&mut self.m.buf);
        let out = f(self);
        (out, std::mem::replace(&mut self.m.buf, outer))
    }

    /// Appends the instructions of `buf`, as `FixupBuffer`'s `++=`.
    pub fn append(&mut self, buf: Buf) {
        for (inst, pos) in buf.insts.into_iter().zip(buf.positions) {
            self.push(inst, pos);
        }
    }
}
