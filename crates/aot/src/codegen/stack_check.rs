//! The stack overflow check on entry to a function that calls others.

use super::body::FnBuilder;
use super::unwind::may_throw;
use crate::R;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Signature, TrapCode, types};
use cranelift_module::{FuncId, Module};
use nir::{Body, Inst, Next};

const ERROR: &str = "java.lang.StackOverflowError";

/// Whether `body` calls other code.
pub fn calls(body: &Body) -> bool {
    body.insts.iter().any(|i| matches!(i, Inst::Let { op, .. } if may_throw(op)))
}

impl FnBuilder<'_, '_> {
    /// Throws StackOverflowError when the stack pointer is below `scivolo_aot_stack_limit`.
    pub fn stack_check(&mut self) -> R<()> {
        let ptr = self.decls.ptr;
        let gv = self.module.declare_data_in_func(self.decls.stack_limit, self.b.func);
        let at = self.b.ins().symbol_value(ptr, gv);
        let limit = self.b.ins().load(ptr, MemFlagsData::new(), at, 0);
        let sp = self.b.ins().get_stack_pointer(ptr);
        let ok = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, sp, limit);
        let (cont, fail) = (self.b.create_block(), self.b.create_block());
        self.b.ins().brif(ok, cont, &[], fail, &[]);
        self.b.set_cold_block(fail);
        self.b.switch_to_block(fail);
        let overflow = self.overflow()?;
        let overflow = self.module.declare_func_in_func(overflow, self.b.func);
        self.b.ins().call(overflow, &[]);
        self.after_call(&Next::None)?;
        self.b.ins().trap(TrapCode::unwrap_user(2));
        self.b.switch_to_block(cont);
        Ok(())
    }

    /// `scivolo_aot_overflow()`: throws a new StackOverflowError, constructed with the stack's
    /// reserve open, as the VM's `throw_new` does.
    fn overflow(&mut self) -> R<FuncId> {
        let ptr = self.decls.ptr;
        let reserve = self.decls.import(self.module, "scivolo_aot_stack_reserve", &[types::I8], None)?;
        let alloc = self.decls.import(self.module, "scivolo_aot_alloc", &[ptr, ptr], Some(ptr))?;
        let cls = self.reach.link_top(ERROR)?;
        self.reach.allocate(cls);
        let rtti = self.decls.rtti(self.module, self.reach, cls)?;
        let size = self.reach.classes.get(cls).size as i64;
        let init = self.reach.world.member(self.reach.classes.get(cls).name, "RE");
        let init = self.decls.func(self.module, self.reach, init)?;
        let slot = self.decls.exception;
        let sig = Signature::new(self.decls.call_conv);
        self.decls.define_generated(self.module, "scivolo_aot_overflow", sig, |b, module| {
            let reserve = module.declare_func_in_func(reserve, b.func);
            let open = b.ins().iconst(types::I8, 1);
            b.ins().call(reserve, &[open]);
            let rtti = module.declare_data_in_func(rtti, b.func);
            let rtti = b.ins().symbol_value(ptr, rtti);
            let size = b.ins().iconst(ptr, size);
            let alloc = module.declare_func_in_func(alloc, b.func);
            let call = b.ins().call(alloc, &[rtti, size]);
            let error = b.inst_results(call)[0];
            let init = module.declare_func_in_func(init, b.func);
            b.ins().call(init, &[error]);
            let slot = module.declare_data_in_func(slot, b.func);
            let slot = b.ins().symbol_value(ptr, slot);
            let thrown = b.ins().load(ptr, MemFlagsData::new(), slot, 0);
            let thrown = b.ins().select(thrown, thrown, error);
            b.ins().store(MemFlagsData::new(), thrown, slot, 0);
            let close = b.ins().iconst(types::I8, 0);
            b.ins().call(reserve, &[close]);
            b.ins().return_(&[]);
            Ok(())
        })
    }
}
