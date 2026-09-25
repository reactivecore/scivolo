use super::Lowerer;
use crate::interp::code::Instr;
use nir::{Next, Op, Val};

impl Lowerer<'_> {
    pub fn op(&mut self, at: u32, dst: u32, op: &Op, unwind: &Next) -> Instr {
        let unwind = self.handler(unwind);
        match op {
            Op::Bin { bin, ty, l, r } => {
                Instr::Bin { bin: *bin, num: self.num_of(*ty), dst, l: self.arg(*l), r: self.arg(*r), unwind }
            }
            Op::Comp { comp, ty, l, r } => {
                Instr::Comp { comp: *comp, num: self.num_of(*ty), dst, l: self.arg(*l), r: self.arg(*r) }
            }
            Op::Conv { conv, ty, value } => Instr::Conv {
                conv: *conv,
                from: self.val_num(*value),
                to: self.num_of(*ty),
                dst,
                v: self.arg(*value),
            },
            Op::Copy(v) => Instr::Copy { dst, v: self.arg(*v) },
            Op::Call { ptr, args, .. } => match *self.pool.val(*ptr) {
                Val::Global(name, _) => {
                    Instr::CallGlobal { dst, name, site: self.site(), args: self.args(args), unwind }
                }
                _ => Instr::Call { dst, target: self.arg(*ptr), args: self.args(args), unwind },
            },
            Op::Method { obj, sig } => {
                let site = self.site();
                self.site();
                Instr::Method { dst, obj: self.arg(*obj), sig: *sig, site, unwind }
            }
            Op::Module(name) => Instr::Module { dst, name: *name, site: self.site(), unwind },
            Op::Fieldload { ty, obj, name } => {
                Instr::FieldLoad { dst, ty: *ty, obj: self.arg(*obj), name: *name, site: self.site(), unwind }
            }
            Op::Fieldstore { ty, obj, name, value } => Instr::FieldStore {
                ty: *ty,
                obj: self.arg(*obj),
                name: *name,
                v: self.arg(*value),
                site: self.site(),
                unwind,
            },
            Op::Arrayload { ty, arr, idx } => {
                Instr::ArrayLoad { dst, ty: *ty, arr: self.arg(*arr), idx: self.arg(*idx), unwind }
            }
            Op::Arraystore { ty, arr, idx, value } => Instr::ArrayStore {
                ty: *ty,
                arr: self.arg(*arr),
                idx: self.arg(*idx),
                v: self.arg(*value),
                unwind,
            },
            Op::Arraylength(arr) => Instr::ArrayLength { dst, arr: self.arg(*arr), unwind },
            Op::Load { ty, ptr, order: None } => Instr::Load { dst, ty: *ty, ptr: self.arg(*ptr) },
            Op::Store { ty, ptr, value, order: None } => {
                Instr::Store { ty: *ty, ptr: self.arg(*ptr), v: self.arg(*value) }
            }
            Op::Classalloc { name, zone: None } => {
                Instr::Classalloc { dst, name: *name, site: self.site(), unwind }
            }
            Op::Is { ty, obj } => Instr::Is { dst, ty: *ty, obj: self.arg(*obj), site: self.site() },
            _ => Instr::Op { dst, inst: at, unwind },
        }
    }
}
