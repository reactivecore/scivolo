//! Executes the value-producing lowered instructions, using and filling per-site caches.

use super::Flow;
use super::code::{Code, Instr, Num};
use super::memory::{load_mem, store_mem};
use crate::Vm;
use crate::VmError;

impl Vm {
    pub fn compute(&mut self, code: &Code, slots: &mut [u64], instr: &Instr) -> Flow<()> {
        let (dst, v) = match *instr {
            Instr::Nop => return Ok(()),
            Instr::Call { dst, ty, target, ref args, .. } => {
                let target = self.arg(code, slots, target)?;
                (dst, self.call_args(code, slots, target, ty, args)?)
            }
            Instr::CallGlobal { dst, name, site, ref args, .. } => {
                let f = self.cached_func(&code.sites[site as usize], name)?;
                (dst, self.call_direct(code, slots, f, args)?)
            }
            Instr::BoxedEquals { dst, name, site, ref args, .. } => {
                let [.., x, y] = **args else { unreachable!("two operands") };
                let (x, y) = (self.arg(code, slots, x)?, self.arg(code, slots, y)?);
                match self.boxed_equals(x, y) {
                    Some(equal) => (dst, equal as u64),
                    None => {
                        let f = self.cached_func(&code.sites[site as usize], name)?;
                        (dst, self.call_direct(code, slots, f, args)?)
                    }
                }
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
                match cell.get() {
                    0 => (dst, self.cached_module(cell, name)?),
                    obj => (dst, obj),
                }
            }
            Instr::FieldLoad { dst, mem, obj, name, site, .. } => {
                let obj = self.object(code, slots, obj)?;
                let at = obj + self.cached_offset(code, site, name)?;
                (dst, load_mem(at, mem))
            }
            Instr::FieldStore { mem, obj, name, v, site, .. } => {
                let obj = self.object(code, slots, obj)?;
                let at = obj + self.cached_offset(code, site, name)?;
                let v = self.arg(code, slots, v)?;
                store_mem(at, mem, v);
                return Ok(());
            }
            Instr::ArrayLoad { dst, mem, arr, idx, .. } => {
                let (arr, idx) = (self.arg(code, slots, arr)?, self.arg(code, slots, idx)?);
                let at = self.array_elem_sized(arr, idx, mem.size())?;
                (dst, load_mem(at, mem))
            }
            Instr::ArrayStore { mem, arr, idx, v, .. } => {
                let (arr, idx, v) =
                    (self.arg(code, slots, arr)?, self.arg(code, slots, idx)?, self.arg(code, slots, v)?);
                let at = self.array_elem_sized(arr, idx, mem.size())?;
                store_mem(at, mem, v);
                return Ok(());
            }
            Instr::ArrayLength { dst, arr, .. } => {
                let arr = self.arg(code, slots, arr)?;
                (dst, self.array_length(arr)?)
            }
            Instr::Load { dst, mem, ptr } => {
                let ptr = self.arg(code, slots, ptr)?;
                (dst, load_mem(ptr as usize, mem))
            }
            Instr::Store { mem, ptr, v } => {
                let (ptr, v) = (self.arg(code, slots, ptr)?, self.arg(code, slots, v)?);
                store_mem(ptr as usize, mem, v);
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
                let cells = &code.sites[site as usize..site as usize + 4];
                (dst, self.boxed(ty, boxing, v, cells)?)
            }
            Instr::Op { dst, ref op, .. } => {
                if self.funcs.profile.on {
                    self.funcs.profile.fallback(op);
                }
                (dst, self.op(op, slots)?)
            }
            _ => unreachable!("control and arithmetic instructions run in `run`"),
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
