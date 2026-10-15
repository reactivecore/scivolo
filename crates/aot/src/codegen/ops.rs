//! Operations: arithmetic, comparisons, conversions and calls; objects are in `objects`.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, TrapCode, Value, types};
use cranelift_module::Module;
use nir::{Bin, Comp, Conv, Op, Val};

impl FnBuilder<'_, '_> {
    /// Translates `op`, the operation of local `id`; its value, if any.
    pub fn op(&mut self, id: u64, op: &Op) -> R<Option<Value>> {
        Ok(Some(match op {
            Op::Call { ty, ptr, args } => return self.call(*ty, *ptr, args),
            Op::Bin { bin, l, r, .. } => {
                let (l, r) = (self.val(*l)?, self.val(*r)?);
                self.bin(*bin, l, r)?
            }
            Op::Comp { comp, l, r, .. } => {
                let (l, r) = (self.val(*l)?, self.val(*r)?);
                match comp_cc(*comp) {
                    Ok(cc) => self.b.ins().icmp(cc, l, r),
                    Err(cc) => self.b.ins().fcmp(cc, l, r),
                }
            }
            Op::Conv { conv, ty, value } => {
                let v = self.val(*value)?;
                let to = self.ty(*ty)?;
                self.conv(*conv, v, to)?
            }
            Op::Copy(v) => self.val(*v)?,
            op => return self.object_op(id, op),
        }))
    }

    fn call(&mut self, ty: nir::TypeId, ptr: nir::ValId, args: &[nir::ValId]) -> R<Option<Value>> {
        let sig = self.decls.signature(&self.reach.world.pool, ty)?;
        if args.len() != sig.params.len() {
            return Err("call arity".into());
        }
        let mut values = Vec::with_capacity(args.len());
        for (&a, p) in args.iter().zip(&sig.params) {
            values.push(self.arg(a, p.value_type)?);
        }
        let direct = match *self.reach.world.pool.val(ptr) {
            Val::Global(g, _) => Some(self.decls.func(self.module, self.reach, g)?),
            Val::Local(l, _) => self.methods.get(&l.0).copied(),
            _ => None,
        };
        let call = match direct {
            Some(f) => {
                let fref = self.module.declare_func_in_func(f, self.b.func);
                self.b.ins().call(fref, &values)
            }
            None => {
                let callee = self.val(ptr)?;
                let sigref = self.b.import_signature(sig);
                self.b.ins().call_indirect(sigref, callee, &values)
            }
        };
        Ok(self.b.inst_results(call).first().copied())
    }

    fn bin(&mut self, bin: Bin, l: Value, r: Value) -> R<Value> {
        let ins = self.b.ins();
        Ok(match bin {
            Bin::Iadd => ins.iadd(l, r),
            Bin::Isub => ins.isub(l, r),
            Bin::Imul => ins.imul(l, r),
            Bin::Fadd => ins.fadd(l, r),
            Bin::Fsub => ins.fsub(l, r),
            Bin::Fmul => ins.fmul(l, r),
            Bin::Fdiv => ins.fdiv(l, r),
            Bin::Shl => ins.ishl(l, r),
            Bin::Lshr => ins.ushr(l, r),
            Bin::Ashr => ins.sshr(l, r),
            Bin::And => ins.band(l, r),
            Bin::Or => ins.bor(l, r),
            Bin::Xor => ins.bxor(l, r),
            Bin::Sdiv | Bin::Srem | Bin::Udiv | Bin::Urem => return self.div(bin, l, r),
            Bin::Frem => return Err("frem not supported yet".into()),
        })
    }

    /// Division and remainder; division by zero traps, `MIN / -1` wraps.
    fn div(&mut self, bin: Bin, l: Value, r: Value) -> R<Value> {
        // Temporary until #113 aot 3 (slow paths): division by zero calls throwDivisionByZero.
        self.b.ins().trapz(r, TrapCode::INTEGER_DIVISION_BY_ZERO);
        Ok(match bin {
            Bin::Udiv => self.b.ins().udiv(l, r),
            Bin::Urem => self.b.ins().urem(l, r),
            _ => {
                let t = self.b.func.dfg.value_type(r);
                let minus_one = self.b.ins().icmp_imm_s(IntCC::Equal, r, -1);
                let one = self.b.ins().iconst(t, 1);
                let safe = self.b.ins().select(minus_one, one, r);
                let (q, special) = if bin == Bin::Sdiv {
                    (self.b.ins().sdiv(l, safe), self.b.ins().ineg(l))
                } else {
                    (self.b.ins().srem(l, safe), self.b.ins().iconst(t, 0))
                };
                self.b.ins().select(minus_one, special, q)
            }
        })
    }

    fn conv(&mut self, conv: Conv, v: Value, to: types::Type) -> R<Value> {
        let from = self.b.func.dfg.value_type(v);
        let ins = self.b.ins();
        Ok(match conv {
            Conv::Sext | Conv::SSizeCast if to.bits() > from.bits() => ins.sextend(to, v),
            Conv::Zext | Conv::ZSizeCast if to.bits() > from.bits() => ins.uextend(to, v),
            Conv::Trunc | Conv::Sext | Conv::SSizeCast | Conv::Zext | Conv::ZSizeCast => {
                if to.bits() < from.bits() { ins.ireduce(to, v) } else { v }
            }
            Conv::Fptrunc => ins.fdemote(to, v),
            Conv::Fpext => ins.fpromote(to, v),
            Conv::Fptoui if to.bits() >= 32 => ins.fcvt_to_uint_sat(to, v),
            Conv::Fptosi if to.bits() >= 32 => ins.fcvt_to_sint_sat(to, v),
            Conv::Uitofp => ins.fcvt_from_uint(to, v),
            Conv::Sitofp => ins.fcvt_from_sint(to, v),
            Conv::Ptrtoint | Conv::Inttoptr | Conv::Bitcast if from == to => v,
            conv => return Err(format!("conversion not supported yet: {conv:?} {from} to {to}")),
        })
    }
}

/// The condition code of `comp`: integer, or floating point as the error side.
fn comp_cc(comp: Comp) -> Result<IntCC, FloatCC> {
    Ok(match comp {
        Comp::Ieq => IntCC::Equal,
        Comp::Ine => IntCC::NotEqual,
        Comp::Ugt => IntCC::UnsignedGreaterThan,
        Comp::Uge => IntCC::UnsignedGreaterThanOrEqual,
        Comp::Ult => IntCC::UnsignedLessThan,
        Comp::Ule => IntCC::UnsignedLessThanOrEqual,
        Comp::Sgt => IntCC::SignedGreaterThan,
        Comp::Sge => IntCC::SignedGreaterThanOrEqual,
        Comp::Slt => IntCC::SignedLessThan,
        Comp::Sle => IntCC::SignedLessThanOrEqual,
        Comp::Feq => return Err(FloatCC::Equal),
        Comp::Fne => return Err(FloatCC::NotEqual),
        Comp::Fgt => return Err(FloatCC::GreaterThan),
        Comp::Fge => return Err(FloatCC::GreaterThanOrEqual),
        Comp::Flt => return Err(FloatCC::LessThan),
        Comp::Fle => return Err(FloatCC::LessThanOrEqual),
    })
}
