use super::Lowerer;
use crate::interp::code::{Arg, Instr, Mem};
use crate::interp::ops::boxing::is_boxed_equals;
use crate::interp::ops::int::{IntBin, Test};
use crate::layout;
use nir::{Next, Op, Type, TypeId, Val, ValId};

impl Lowerer<'_> {
    /// `op`, whose result is local `local` in slot `dst`. What no instruction covers stays a NIR
    /// operation, whose operands `lower` later renumbers to slots.
    pub fn op(&mut self, local: u64, dst: u32, op: &Op, unwind: &Next) -> Instr {
        let unwind = self.handler(unwind);
        match op {
            Op::Bin { bin, ty, l, r } => {
                let (num, l, r) = (self.num_of(*ty), self.arg(*l), self.arg(*r));
                let divisor = match r {
                    Arg::Imm(v) => Some(v as u64),
                    _ => None,
                };
                let eager = self.is_known(l) && self.is_known(r);
                match IntBin::of(*bin, num, divisor).filter(|_| eager) {
                    Some(op) => Instr::Int { op, dst, l, r },
                    None => Instr::Bin { bin: *bin, num, dst, l, r, unwind },
                }
            }
            Op::Comp { comp, ty, l, r } => {
                Instr::Comp { test: Test::of(*comp, self.num_of(*ty)), dst, l: self.arg(*l), r: self.arg(*r) }
            }
            Op::Conv { conv, ty, value } => Instr::Conv {
                conv: *conv,
                from: self.val_num(*value),
                to: self.num_of(*ty),
                dst,
                v: self.arg(*value),
            },
            Op::Copy(v) => Instr::Copy { dst, v: self.arg(*v) },
            Op::Call { ty, ptr, args } => match *self.pool.val(*ptr) {
                Val::Global(name, _) if self.is_variadic(*ty) => {
                    let args = args.iter().map(|&a| (self.arg(a), self.val_num(a))).collect();
                    let ret = match self.pool.ty(*ty) {
                        Type::Function(_, ret) => self.num_of(*ret),
                        _ => super::Num::I64,
                    };
                    Instr::CallVariadic { dst, name, args, ret, unwind }
                }
                Val::Global(name, _) if is_boxed_equals(self.pool, name) => {
                    Instr::BoxedEquals { dst, name, site: self.site(), args: self.args(args), unwind }
                }
                Val::Global(name, _) => {
                    Instr::CallGlobal { dst, name, site: self.site(), args: self.args(args), unwind }
                }
                Val::Local(l, _)
                    if self.fusable.get(&l.0).is_some_and(|(obj, _)| args.first() == Some(obj)) =>
                {
                    let sig = self.fusable[&l.0].1;
                    let site = self.site();
                    self.site();
                    Instr::Invoke { dst, sig, site, args: self.args(args), unwind }
                }
                _ => Instr::Call { dst, ty: *ty, target: self.arg(*ptr), args: self.args(args), unwind },
            },
            Op::Method { .. } if self.fusable.contains_key(&local) => Instr::Nop,
            Op::Method { obj, sig } => {
                let site = self.site();
                self.site();
                Instr::Method { dst, obj: self.arg(*obj), sig: *sig, site, unwind }
            }
            Op::Module(name) => Instr::Module { dst, name: *name, site: self.site(), unwind },
            Op::Fieldload { ty, obj, name } => {
                let mem = Mem::of(self.pool, *ty);
                Instr::FieldLoad { dst, mem, obj: self.arg(*obj), name: *name, site: self.site(), unwind }
            }
            Op::Fieldstore { ty, obj, name, value } => Instr::FieldStore {
                mem: Mem::of(self.pool, *ty),
                obj: self.arg(*obj),
                name: *name,
                v: self.arg(*value),
                site: self.site(),
                unwind,
            },
            Op::Arrayload { ty, arr, idx } => {
                let mem = Mem::of(self.pool, *ty);
                Instr::ArrayLoad { dst, mem, arr: self.arg(*arr), idx: self.arg(*idx), unwind }
            }
            Op::Arraystore { ty, arr, idx, value } => Instr::ArrayStore {
                mem: Mem::of(self.pool, *ty),
                arr: self.arg(*arr),
                idx: self.arg(*idx),
                v: self.arg(*value),
                unwind,
            },
            Op::Arraylength(arr) => Instr::ArrayLength { dst, arr: self.arg(*arr), unwind },
            Op::Load { ty, ptr, .. } => {
                Instr::Load { dst, mem: Mem::of(self.pool, *ty), ptr: self.arg(*ptr) }
            }
            Op::Store { ty, ptr, value, .. } => {
                Instr::Store { mem: Mem::of(self.pool, *ty), ptr: self.arg(*ptr), v: self.arg(*value) }
            }
            Op::Var(_) if self.slot_vars.contains(&local) => Instr::Copy { dst, v: Arg::Imm(0) },
            Op::Varload(cell) if self.is_slot_var(*cell) => Instr::Copy { dst, v: self.arg(*cell) },
            Op::Varstore { slot: cell, value } if self.is_slot_var(*cell) => match self.arg(*cell) {
                Arg::Slot(cell) => Instr::Copy { dst: cell, v: self.arg(*value) },
                _ => Instr::Op { dst, op: Box::new(op.clone()), unwind },
            },
            Op::SizeOf(ty) if !matches!(self.pool.ty(*ty), Type::Ref { .. }) => {
                Instr::Copy { dst, v: Arg::Imm(layout::size_of(self.pool, *ty)) }
            }
            Op::AlignmentOf(ty) => Instr::Copy { dst, v: Arg::Imm(layout::align_of(self.pool, *ty)) },
            Op::Elem { ty, ptr, indexes } => match self.elem(dst, *ty, *ptr, indexes) {
                Some(instr) => instr,
                None => Instr::Op { dst, op: Box::new(op.clone()), unwind },
            },
            Op::Classalloc { name, zone: None } => {
                Instr::Classalloc { dst, name: *name, site: self.site(), unwind }
            }
            Op::Is { ty, obj } => Instr::Is { dst, ty: *ty, obj: self.arg(*obj), site: self.site() },
            Op::As { ty, obj } if matches!(self.pool.ty(*ty), Type::Ref { .. } | Type::Array { .. }) => {
                Instr::As { dst, ty: *ty, obj: self.arg(*obj), site: self.site(), unwind }
            }
            Op::Box { ty, obj } | Op::Unbox { ty, obj } => {
                let site = self.site();
                self.sites += 3;
                let boxing = matches!(op, Op::Box { .. });
                Instr::Box { dst, ty: *ty, boxing, v: self.arg(*obj), site, unwind }
            }
            _ => Instr::Op { dst, op: Box::new(op.clone()), unwind },
        }
    }

    fn is_variadic(&self, ty: TypeId) -> bool {
        match self.pool.ty(ty) {
            Type::Function(args, _) => args.iter().any(|t| matches!(self.pool.ty(*t), Type::Vararg)),
            _ => false,
        }
    }

    fn is_slot_var(&self, cell: ValId) -> bool {
        matches!(self.pool.val(cell), Val::Local(l, _) if self.slot_vars.contains(&l.0))
    }

    /// `elem[ty] ptr, i, c1, c2...` with constant `c`s, as `ptr + i * size + offset`.
    fn elem(&mut self, dst: u32, ty: TypeId, ptr: ValId, indexes: &[ValId]) -> Option<Instr> {
        let (&first, rest) = indexes.split_first()?;
        let (mut offset, mut inner) = (0i64, ty);
        for &i in rest {
            let i = match self.pool.val(i) {
                Val::Int(i) => *i as i64,
                Val::Long(i) => *i,
                Val::Size(i) => *i as i64,
                _ => return None,
            };
            match self.pool.ty(inner) {
                Type::StructValue(tys) => {
                    offset += layout::struct_layout(self.pool, tys).1[i as usize] as i64;
                    inner = tys[i as usize];
                }
                Type::ArrayValue(elem, _) => {
                    offset += i * layout::size_of(self.pool, *elem) as i64;
                    inner = *elem;
                }
                _ => return None,
            }
        }
        Some(Instr::Elem {
            dst,
            base: self.arg(ptr),
            idx: self.arg(first),
            idx_num: self.val_num(first),
            scale: layout::size_of(self.pool, ty),
            offset,
        })
    }
}
