// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/serialization/BinarySerializer.scala
use super::{Out, Writer};
use crate::nir::{BINS, COMPS, CONVS, Cond, Inst, Next, ORDERS, Op, Param, PosId};
use crate::tags::{cond, inst, next, op};

/// The tag of `item`: its index in `table` plus one.
fn table<T: PartialEq>(table: &[T], item: &T) -> u8 {
    table.iter().position(|t| t == item).expect("an item of the table") as u8 + 1
}

impl Writer<'_> {
    pub fn inst(&mut self, out: &mut Out, inst: &Inst, pos: PosId) {
        let tag = match inst {
            Inst::Label(..) => inst::LABEL,
            Inst::Let { unwind: Next::None, .. } => inst::LET,
            Inst::Let { .. } => inst::LET_UNWIND,
            Inst::Ret(_) => inst::RET,
            Inst::Jump(_) => inst::JUMP,
            Inst::If(..) => inst::IF,
            Inst::Switch(..) => inst::SWITCH,
            Inst::Throw(..) => inst::THROW,
            Inst::Unreachable(_) => inst::UNREACHABLE,
            Inst::LinktimeIf(..) => inst::LINKTIME_IF,
        };
        out.u8(tag);
        self.position(out, pos);
        match inst {
            Inst::Label(id, params) => {
                self.local(out, *id);
                out.uleb(params.len() as u64);
                params.iter().for_each(|p| self.param(out, p));
            }
            Inst::Let { id, op, unwind, scope } => {
                self.local(out, *id);
                self.op(out, op);
                if !matches!(unwind, Next::None) {
                    self.next(out, unwind);
                }
                out.uleb(*scope as u64);
            }
            Inst::Ret(v) => self.val(out, *v),
            Inst::Jump(n) | Inst::Unreachable(n) => self.next(out, n),
            Inst::If(v, then, els) => {
                self.val(out, *v);
                self.next(out, then);
                self.next(out, els);
            }
            Inst::Switch(v, default, cases) => {
                self.val(out, *v);
                self.next(out, default);
                out.uleb(cases.len() as u64);
                cases.iter().for_each(|c| self.next(out, c));
            }
            Inst::Throw(v, unwind) => {
                self.val(out, *v);
                self.next(out, unwind);
            }
            Inst::LinktimeIf(c, then, els) => {
                self.cond(out, c);
                self.next(out, then);
                self.next(out, els);
            }
        }
    }

    fn param(&mut self, out: &mut Out, p: &Param) {
        self.local(out, p.id);
        self.ty(out, p.ty);
    }

    fn next(&mut self, out: &mut Out, n: &Next) {
        match n {
            Next::None => out.u8(next::NONE),
            Next::Unwind(p, n) => {
                out.u8(next::UNWIND);
                self.param(out, p);
                self.next(out, n);
            }
            Next::Case(v, n) => {
                out.u8(next::CASE);
                self.val(out, *v);
                self.next(out, n);
            }
            Next::Label(id, args) => {
                out.u8(next::LABEL);
                self.local(out, *id);
                self.vals(out, args);
            }
        }
    }

    fn cond(&mut self, out: &mut Out, c: &Cond) {
        match c {
            Cond::Simple { property, comp, value, pos } => {
                out.u8(cond::SIMPLE);
                self.string(out, *property);
                out.u8(table(&COMPS, comp));
                self.val(out, *value);
                self.position(out, *pos);
            }
            Cond::Complex { bin, left, right, pos } => {
                out.u8(cond::COMPLEX);
                out.u8(table(&BINS, bin));
                self.cond(out, left);
                self.cond(out, right);
                self.position(out, *pos);
            }
        }
    }

    fn op(&mut self, out: &mut Out, o: &Op) {
        match o {
            Op::Call { ty, ptr, args } => {
                out.u8(op::CALL);
                self.ty(out, *ty);
                self.val(out, *ptr);
                self.vals(out, args);
            }
            Op::Load { ty, ptr, order } => {
                out.u8(if order.is_some() { op::LOAD_ATOMIC } else { op::LOAD });
                self.ty(out, *ty);
                self.val(out, *ptr);
                if let Some(o) = order {
                    out.u8(table(&ORDERS, o));
                }
            }
            Op::Store { ty, ptr, value, order } => {
                out.u8(if order.is_some() { op::STORE_ATOMIC } else { op::STORE });
                self.ty(out, *ty);
                self.val(out, *ptr);
                self.val(out, *value);
                if let Some(o) = order {
                    out.u8(table(&ORDERS, o));
                }
            }
            Op::Elem { ty, ptr, indexes } => {
                out.u8(op::ELEM);
                self.ty(out, *ty);
                self.val(out, *ptr);
                self.vals(out, indexes);
            }
            Op::Extract { aggr, indexes } => {
                out.u8(op::EXTRACT);
                self.val(out, *aggr);
                out.uleb(indexes.len() as u64);
                indexes.iter().for_each(|&i| out.sleb(i as i64));
            }
            Op::Insert { aggr, value, indexes } => {
                out.u8(op::INSERT);
                self.val(out, *aggr);
                self.val(out, *value);
                out.uleb(indexes.len() as u64);
                indexes.iter().for_each(|&i| out.sleb(i as i64));
            }
            Op::Stackalloc { ty, n } => {
                out.u8(op::STACKALLOC);
                self.ty(out, *ty);
                self.val(out, *n);
            }
            Op::Bin { bin, ty, l, r } => {
                out.u8(op::BIN);
                out.u8(table(&BINS, bin));
                self.ty(out, *ty);
                self.val(out, *l);
                self.val(out, *r);
            }
            Op::Comp { comp, ty, l, r } => {
                out.u8(op::COMP);
                out.u8(table(&COMPS, comp));
                self.ty(out, *ty);
                self.val(out, *l);
                self.val(out, *r);
            }
            Op::Conv { conv, ty, value } => {
                out.u8(op::CONV);
                out.u8(table(&CONVS, conv));
                self.ty(out, *ty);
                self.val(out, *value);
            }
            _ => self.object_op(out, o),
        }
    }
}
