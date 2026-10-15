//! Slow paths: a failed null, bounds, division or cast check calls the runtime's `throwX`, then
//! leaves through the instruction's unwind edge like any call that threw.

use super::RUNTIME;
use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, InstBuilder, TrapCode, Value};
use cranelift_module::Module;
use nir::{Next, Type, Val, ValId};

pub const NULL_POINTER: &str = "D16throwNullPointernEO";
pub const OUT_OF_BOUNDS: &str = "D16throwOutOfBoundsiinEO";
pub const DIVISION_BY_ZERO: &str = "D19throwDivisionByZeronEO";
pub const CLASS_CAST: &str = "D14throwClassCastR_R_nEO";

impl FnBuilder<'_, '_> {
    /// Continues where `ok` is nonzero; else throws with `throw(args)` of the runtime. Without
    /// arguments or a handler, the throwing block is shared by the function's checks.
    pub fn check(&mut self, ok: Value, throw: &'static str, args: &[Value]) -> R<()> {
        let shared = args.is_empty() && matches!(self.unwind, Next::None);
        let cont = self.b.create_block();
        if let Some(&fail) = self.fail_blocks.get(throw).filter(|_| shared) {
            self.b.ins().brif(ok, cont, &[], fail, &[]);
            self.b.switch_to_block(cont);
            return Ok(());
        }
        let fail = self.b.create_block();
        if shared {
            self.fail_blocks.insert(throw, fail);
        }
        self.b.ins().brif(ok, cont, &[], fail, &[]);
        self.fill_fail(fail, throw, args)?;
        self.b.switch_to_block(cont);
        Ok(())
    }

    fn fill_fail(&mut self, fail: Block, throw: &str, args: &[Value]) -> R<()> {
        self.b.switch_to_block(fail);
        let runtime = self.reach.link_top(RUNTIME)?;
        let load = self.decls.module_load(self.module, self.reach, runtime)?;
        let g = self.reach.world.member(self.reach.classes.get(runtime).name, throw);
        let f = self.decls.func(self.module, self.reach, g)?;
        let load = self.module.declare_func_in_func(load, self.b.func);
        let call = self.b.ins().call(load, &[]);
        let instance = self.b.inst_results(call)[0];
        let fref = self.module.declare_func_in_func(f, self.b.func);
        let all: Vec<Value> = std::iter::once(instance).chain(args.iter().copied()).collect();
        self.b.ins().call(fref, &all);
        let unwind = self.unwind.clone();
        self.after_call(&unwind)?;
        self.b.ins().trap(TrapCode::unwrap_user(2));
        Ok(())
    }

    /// Whether NIR value `v` may be null: not a constant, a fresh object or of a non-nullable type.
    pub fn may_be_null(&self, v: ValId) -> bool {
        let pool = &self.reach.world.pool;
        match pool.val(v) {
            Val::String(_) | Val::ClassOf(_) | Val::Const(_) => false,
            Val::Local(l, ty) => {
                let nullable = match pool.ty(*ty) {
                    Type::Ref { nullable, .. } | Type::Array { nullable, .. } => *nullable,
                    _ => true,
                };
                nullable && !self.non_null.contains(&l.0)
            }
            _ => true,
        }
    }

    /// Throws NullPointerException if `obj`, the value of `v`, is null and `v` may be null.
    pub fn null_check_of(&mut self, v: ValId, obj: Value) -> R<()> {
        match self.may_be_null(v) {
            true => self.null_check(obj),
            false => Ok(()),
        }
    }

    pub fn null_check(&mut self, obj: Value) -> R<()> {
        let ok = self.b.ins().icmp_imm_s(IntCC::NotEqual, obj, 0);
        self.check(ok, NULL_POINTER, &[])
    }

    /// Throws unless `0 <= idx < len`.
    pub fn bounds_check(&mut self, idx: Value, len: Value) -> R<()> {
        let ok = self.b.ins().icmp(IntCC::UnsignedLessThan, idx, len);
        self.check(ok, OUT_OF_BOUNDS, &[idx, len])
    }
}
