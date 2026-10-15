//! Operations: arithmetic, comparisons, conversions and calls; objects are in `objects`.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, types};
use cranelift_module::Module;
use nir::{Bin, Comp, Conv, Op, Val};

impl FnBuilder<'_, '_> {
    /// Translates `op`, the operation of local `id`; its value, if any.
    pub fn op(&mut self, id: u64, op: &Op) -> R<Option<Value>> {
        Ok(Some(match op {
            Op::Call { ty, ptr, args } => match self.intrinsic(*ptr, args)? {
                Some(v) => v,
                None => return self.call(*ty, *ptr, args),
            },
            Op::Bin { bin, l, r, .. } => {
                let nonzero = match *self.reach.world.pool.val(*r) {
                    Val::Int(x) => x != 0,
                    Val::Long(x) => x != 0,
                    _ => false,
                };
                let (l, r) = (self.val(*l)?, self.val(*r)?);
                self.bin(*bin, l, r, nonzero)?
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
            op => return self.memory_op(id, op),
        }))
    }

    fn call(&mut self, ty: nir::TypeId, ptr: nir::ValId, args: &[nir::ValId]) -> R<Option<Value>> {
        if self.reach.world.pool.is_variadic(ty) {
            let Val::Global(g, _) = *self.reach.world.pool.val(ptr) else {
                return Err("a variadic call through a pointer".into());
            };
            return self.variadic_call(g, ty, args);
        }
        let target = match *self.reach.world.pool.val(ptr) {
            Val::Global(g, _) => Some(g),
            Val::Local(l, _) => self.linktime_methods.get(&l.0).copied(),
            _ => None,
        };
        let linktime = target.filter(|&g| self.is_linktime_resolved(g));
        if let Some(g) = linktime
            && let Ok(v) = self.linktime_value(g)
        {
            return Ok(Some(v));
        }
        let sig = self.decls.signature(&self.reach.world.pool, ty)?;
        if args.len() != sig.params.len() {
            return Err("call arity".into());
        }
        let nir::Type::Function(params, _) = self.reach.world.pool.ty(ty).clone() else {
            return Err("a call of a non-function type".into());
        };
        let mut values = Vec::with_capacity(args.len());
        for (&a, p) in args.iter().zip(params) {
            values.push(self.arg_of(a, p)?);
        }
        let direct = match *self.reach.world.pool.val(ptr) {
            Val::Global(g, _) => Some(self.decls.func(self.module, self.reach, g)?),
            Val::Local(l, _) => self.methods.get(&l.0).copied(),
            _ => None,
        };
        let call = match direct {
            Some(f) => {
                let decl = self.module.declarations().get_function_decl(f);
                if linktime.is_some() && decl.signature.params.len() + 1 == values.len() {
                    values.remove(0);
                }
                if decl.signature.params.len() != values.len() {
                    let name = decl.linkage_name(f).into_owned();
                    return Err(format!("{name} takes {} arguments, the call passes {}", decl.signature.params.len(), values.len()));
                }
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

    /// `l bin r`; `nonzero` when `r` is a constant other than zero.
    fn bin(&mut self, bin: Bin, l: Value, r: Value, nonzero: bool) -> R<Value> {
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
            Bin::Sdiv | Bin::Srem | Bin::Udiv | Bin::Urem => return self.div(bin, l, r, nonzero),
            Bin::Frem => return self.frem(l, r),
        })
    }

    /// The floating-point remainder of `l / r`, through C's `fmod`.
    fn frem(&mut self, l: Value, r: Value) -> R<Value> {
        let t = self.b.func.dfg.value_type(l);
        let name = if t == types::F32 { "scalai_aot_frem_f32" } else { "scalai_aot_frem_f64" };
        let f = self.decls.import(self.module, name, &[t, t], Some(t))?;
        let fref = self.module.declare_func_in_func(f, self.b.func);
        let call = self.b.ins().call(fref, &[l, r]);
        Ok(self.b.inst_results(call)[0])
    }

    /// Division and remainder; division by zero throws, `MIN / -1` wraps.
    fn div(&mut self, bin: Bin, l: Value, r: Value, nonzero: bool) -> R<Value> {
        if !nonzero {
            let ok = self.b.ins().icmp_imm_s(IntCC::NotEqual, r, 0);
            self.check(ok, super::checks::DIVISION_BY_ZERO, &[])?;
        }
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
            Conv::Ptrtoint | Conv::Inttoptr | Conv::Bitcast if from.is_int() && to.is_int() => {
                if to.bits() > from.bits() { ins.uextend(to, v) } else { ins.ireduce(to, v) }
            }
            Conv::Bitcast if from.bits() == to.bits() => ins.bitcast(to, MemFlagsData::new(), v),
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
