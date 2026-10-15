//! Slow paths: a failed null, bounds, division or cast check calls the runtime's `throwX`, then
//! leaves through the instruction's unwind edge like any call that threw.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, TrapCode, Value};
use cranelift_module::Module;

const RUNTIME: &str = "scala.scalanative.runtime.package$";
pub const NULL_POINTER: &str = "D16throwNullPointernEO";
pub const OUT_OF_BOUNDS: &str = "D16throwOutOfBoundsiinEO";
pub const DIVISION_BY_ZERO: &str = "D19throwDivisionByZeronEO";
pub const CLASS_CAST: &str = "D14throwClassCastR_R_nEO";

impl FnBuilder<'_, '_> {
    /// Continues where `ok` is nonzero; else throws with `throw(args)` of the runtime.
    pub fn check(&mut self, ok: Value, throw: &str, args: &[Value]) -> R<()> {
        let (fail, cont) = (self.b.create_block(), self.b.create_block());
        self.b.ins().brif(ok, cont, &[], fail, &[]);
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
        self.b.switch_to_block(cont);
        Ok(())
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
