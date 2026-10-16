//! The stack overflow check on entry to a function that calls others.

use super::RUNTIME;
use super::body::FnBuilder;
use super::unwind::may_throw;
use crate::R;
use cranelift_codegen::Context;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Signature, TrapCode, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use nir::{Body, Inst, Next};

const THROW: &str = "D30throwPendingStackOverflowErroruEO";

/// Whether `body` calls other code.
pub fn calls(body: &Body) -> bool {
    body.insts.iter().any(|i| matches!(i, Inst::Let { op, .. } if may_throw(op)))
}

impl FnBuilder<'_, '_> {
    /// Throws StackOverflowError when the stack pointer is below `scalai_aot_stack_limit`.
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

    /// `scalai_aot_overflow()`: throws StackOverflowError, constructed with the stack's reserve
    /// open.
    fn overflow(&mut self) -> R<FuncId> {
        let sig = Signature::new(self.decls.call_conv);
        let id = self
            .module
            .declare_function("scalai_aot_overflow", Linkage::Local, &sig)
            .map_err(|e| e.to_string())?;
        if std::mem::replace(&mut self.decls.overflow_defined, true) {
            return Ok(id);
        }
        let reserve = self.decls.import(self.module, "scalai_aot_stack_reserve", &[types::I8], None)?;
        let runtime = self.reach.link_top(RUNTIME)?;
        let load = self.decls.module_load(self.module, self.reach, runtime)?;
        let g = self.reach.world.member(self.reach.classes.get(runtime).name, THROW);
        let throw = self.decls.func(self.module, self.reach, g)?;
        let mut ctx = Context::new();
        ctx.func.signature = sig;
        let mut fb = FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
        let block = b.create_block();
        b.switch_to_block(block);
        let reserve = self.module.declare_func_in_func(reserve, b.func);
        let open = b.ins().iconst(types::I8, 1);
        b.ins().call(reserve, &[open]);
        let load = self.module.declare_func_in_func(load, b.func);
        let call = b.ins().call(load, &[]);
        let instance = b.inst_results(call)[0];
        let throw = self.module.declare_func_in_func(throw, b.func);
        b.ins().call(throw, &[instance]);
        let close = b.ins().iconst(types::I8, 0);
        b.ins().call(reserve, &[close]);
        b.ins().return_(&[]);
        b.seal_all_blocks();
        b.finalize(self.module.target_config());
        self.module.define_function(id, &mut ctx).map_err(|e| format!("scalai_aot_overflow: {e:?}"))?;
        Ok(id)
    }
}
