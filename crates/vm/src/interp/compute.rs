//! Executes the value-producing lowered instructions, using and filling per-site caches.

use super::Flow;
use super::code::{Code, Handler, Instr, Num};
use super::exec::body;
use super::ops::arith;
use crate::Vm;
use crate::VmError;
use nir::Inst;

impl Instr {
    pub fn unwind(&self) -> &Handler {
        match self {
            Instr::Bin { unwind, .. }
            | Instr::Call { unwind, .. }
            | Instr::CallGlobal { unwind, .. }
            | Instr::Invoke { unwind, .. }
            | Instr::CallVariadic { unwind, .. }
            | Instr::Method { unwind, .. }
            | Instr::Module { unwind, .. }
            | Instr::FieldLoad { unwind, .. }
            | Instr::FieldStore { unwind, .. }
            | Instr::ArrayLoad { unwind, .. }
            | Instr::ArrayStore { unwind, .. }
            | Instr::ArrayLength { unwind, .. }
            | Instr::Classalloc { unwind, .. }
            | Instr::As { unwind, .. }
            | Instr::Box { unwind, .. }
            | Instr::Op { unwind, .. }
            | Instr::Throw { unwind, .. } => unwind,
            _ => &None,
        }
    }
}

impl Vm {
    pub fn compute(&mut self, code: &Code, slots: &mut [u64], instr: &Instr) -> Flow<()> {
        let (dst, v) = match *instr {
            Instr::Bin { bin, num, dst, l, r, .. } => {
                let (l, r) = (self.arg(code, slots, l)?, self.arg(code, slots, r)?);
                (dst, self.bin(bin, num, l, r)?)
            }
            Instr::Comp { comp, num, dst, l, r } => {
                let (l, r) = (self.arg(code, slots, l)?, self.arg(code, slots, r)?);
                (dst, arith::comp(comp, num, l, r) as u64)
            }
            Instr::Conv { conv, from, to, dst, v } => {
                (dst, arith::conv(conv, from, to, self.arg(code, slots, v)?))
            }
            Instr::Copy { dst, v } => (dst, self.arg(code, slots, v)?),
            Instr::Nop => return Ok(()),
            Instr::Call { dst, target, ref args, .. } => {
                let target = self.arg(code, slots, target)?;
                (dst, self.call_args(code, slots, target, args)?)
            }
            Instr::CallGlobal { dst, name, site, ref args, .. } => {
                let f = self.cached_func(&code.sites[site as usize], name)?;
                (dst, self.call_direct(code, slots, f, args)?)
            }
            Instr::Invoke { dst, sig, site, ref args, .. } => {
                (dst, self.invoke(code, slots, sig, site, args)?)
            }
            Instr::CallVariadic { dst, name, ref args, ret, .. } => {
                let values = args
                    .iter()
                    .map(|&(a, n)| Ok((self.arg(code, slots, a)?, n)))
                    .collect::<Flow<Vec<_>>>()?;
                (dst, self.call_variadic(name, &values, ret)?)
            }
            Instr::Method { dst, obj, sig, site, .. } => {
                let obj = self.object(code, slots, obj)?;
                let cls = self.class_of(obj) as u64 + 1;
                let (seen, value) = (&code.sites[site as usize], &code.sites[site as usize + 1]);
                if seen.get() != cls {
                    let f = self.virtual_method(obj, sig)?;
                    value.set(self.callable(f)?);
                    seen.set(cls);
                }
                (dst, value.get())
            }
            Instr::Module { dst, name, site, .. } => {
                let cell = &code.sites[site as usize];
                if cell.get() == 0 {
                    cell.set(self.module(name)? as u64);
                }
                (dst, cell.get())
            }
            Instr::FieldLoad { dst, ty, obj, name, site, .. } => {
                let obj = self.object(code, slots, obj)?;
                let at = obj + self.cached_offset(code, site, name)?;
                (dst, self.load_ty(at, ty))
            }
            Instr::FieldStore { ty, obj, name, v, site, .. } => {
                let obj = self.object(code, slots, obj)?;
                let at = obj + self.cached_offset(code, site, name)?;
                let v = self.arg(code, slots, v)?;
                self.store_ty(at, ty, v);
                return Ok(());
            }
            Instr::ArrayLoad { dst, ty, arr, idx, .. } => {
                let (arr, idx) = (self.arg(code, slots, arr)?, self.arg(code, slots, idx)?);
                let at = self.array_elem(arr, idx, ty)?;
                (dst, self.load_ty(at, ty))
            }
            Instr::ArrayStore { ty, arr, idx, v, .. } => {
                let (arr, idx, v) =
                    (self.arg(code, slots, arr)?, self.arg(code, slots, idx)?, self.arg(code, slots, v)?);
                let at = self.array_elem(arr, idx, ty)?;
                self.store_ty(at, ty, v);
                return Ok(());
            }
            Instr::ArrayLength { dst, arr, .. } => {
                let arr = self.arg(code, slots, arr)?;
                (dst, self.array_length(arr)?)
            }
            Instr::Load { dst, ty, ptr } => {
                let ptr = self.arg(code, slots, ptr)?;
                (dst, self.load_ty(ptr as usize, ty))
            }
            Instr::Store { ty, ptr, v } => {
                let (ptr, v) = (self.arg(code, slots, ptr)?, self.arg(code, slots, v)?);
                self.store_ty(ptr as usize, ty, v);
                return Ok(());
            }
            Instr::Classalloc { dst, name, site, .. } => {
                let cell = &code.sites[site as usize];
                if cell.get() == 0 {
                    cell.set(self.link(name)? as u64 + 1);
                }
                (dst, self.alloc_object(cell.get() as u32 - 1) as u64)
            }
            Instr::Is { dst, ty, obj, site } => {
                let obj = self.arg(code, slots, obj)? as usize;
                let cell = &code.sites[site as usize];
                if obj != 0 && cell.get() == 0 {
                    let Some(cls) = self.class_of_type(ty)? else {
                        return Err(VmError::Unsupported("type test against a non-class type".into()).into());
                    };
                    cell.set(cls as u64 + 1);
                }
                let is = obj != 0 && self.is_subclass(self.class_of(obj), cell.get() as u32 - 1);
                (dst, is as u64)
            }
            Instr::Elem { dst, base, idx, idx_num, scale, offset } => {
                let (base, idx) = (self.arg(code, slots, base)?, self.arg(code, slots, idx)?);
                let idx = match idx_num {
                    Num::I32 => idx as i32 as i64,
                    Num::I16 => idx as i16 as i64,
                    Num::I8 => idx as i8 as i64,
                    _ => idx as i64,
                };
                (dst, base.wrapping_add_signed(idx * scale as i64 + offset))
            }
            Instr::As { dst, ty, obj, site, .. } => {
                let obj = self.arg(code, slots, obj)?;
                (dst, self.cast(obj, ty, &code.sites[site as usize])?)
            }
            Instr::Box { dst, ty, boxing, v, site, .. } => {
                let v = self.arg(code, slots, v)?;
                let cells = &code.sites[site as usize..site as usize + 2];
                (dst, self.boxed(ty, boxing, v, cells)?)
            }
            Instr::Op { dst, inst, .. } => {
                let insts = body(code).0;
                let Inst::Let { op, .. } = &insts[inst as usize] else { unreachable!("lowered from a let") };
                if self.funcs.profile.on {
                    self.funcs.profile.fallback(op);
                }
                (dst, self.op(op, slots)?)
            }
            _ => unreachable!("control instructions run in `run`"),
        };
        slots[dst as usize] = v;
        Ok(())
    }

    /// The non-null object `a` holds, or a thrown `NullPointerException`.
    fn object(&mut self, code: &Code, slots: &[u64], a: super::code::Arg) -> Flow<usize> {
        match self.arg(code, slots, a)? as usize {
            0 => self.null_pointer(),
            obj => Ok(obj),
        }
    }

    fn cached_offset(&mut self, code: &Code, site: u32, name: nir::GlobalId) -> Flow<usize> {
        let cell = &code.sites[site as usize];
        if cell.get() == 0 {
            cell.set(self.field_offset(name)? as u64 + 1);
        }
        Ok(cell.get() as usize - 1)
    }
}
