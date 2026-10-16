//! Functions the codegen writes without NIR: generated helpers, the test entry wrapper and the
//! body of a function that cannot be compiled.

use super::Decls;
use crate::R;
use crate::blob::Blob;
use cranelift_codegen::Context;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, TrapCode, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

impl Decls {
    /// Local function `name` with signature `sig`, whose body `body` writes into its entry block
    /// on first use.
    pub fn define_generated(
        &mut self,
        module: &mut ObjectModule,
        name: &str,
        sig: Signature,
        body: impl FnOnce(&mut FunctionBuilder, &mut ObjectModule) -> R<()>,
    ) -> R<FuncId> {
        let id = module.declare_function(name, Linkage::Local, &sig).map_err(|e| e.to_string())?;
        if !self.generated.insert(name.to_owned()) {
            return Ok(id);
        }
        let mut ctx = Context::new();
        ctx.func.signature = sig;
        let mut fb = FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
        let block = b.create_block();
        b.append_block_params_for_function_params(block);
        b.switch_to_block(block);
        body(&mut b, module)?;
        b.seal_all_blocks();
        b.finalize(module.target_config());
        module.define_function(id, &mut ctx).map_err(|e| format!("{name}: {e:?}"))?;
        Ok(id)
    }
}

/// A body for the function in `ctx` that reports `message` and stops the program.
pub fn missing(
    module: &mut ObjectModule,
    decls: &mut Decls,
    (ctx, fb): (&mut Context, &mut FunctionBuilderContext),
    message: &str,
) -> R<()> {
    let sig = ctx.func.signature.clone();
    ctx.clear();
    *fb = FunctionBuilderContext::new();
    ctx.func.signature = sig;
    decls.anon += 1;
    let text = module
        .declare_data(&format!("scalai_aot_missing.{}", decls.anon), Linkage::Local, false, false)
        .map_err(|e| e.to_string())?;
    let bytes: Vec<u8> = message.bytes().chain([0]).collect();
    Blob { bytes, ..Default::default() }.define(module, text)?;
    let report = decls.import(module, "scalai_aot_missing", &[decls.ptr], None)?;
    let mut b = FunctionBuilder::new(&mut ctx.func, fb);
    let block = b.create_block();
    b.append_block_params_for_function_params(block);
    b.switch_to_block(block);
    let gv = module.declare_data_in_func(text, b.func);
    let text = b.ins().symbol_value(decls.ptr, gv);
    let report = module.declare_func_in_func(report, b.func);
    b.ins().call(report, &[text]);
    b.ins().trap(TrapCode::unwrap_user(3));
    b.seal_all_blocks();
    b.finalize(module.target_config());
    Ok(())
}

/// `scalai_entry(n: i32) -> i32`, which calls `target`.
pub fn entry_wrapper(module: &mut ObjectModule, decls: &Decls, target: FuncId) -> R<()> {
    let mut sig = Signature::new(decls.call_conv);
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    let id = module.declare_function("scalai_entry", Linkage::Export, &sig).map_err(|e| e.to_string())?;
    let mut ctx = Context::new();
    ctx.func.signature = sig;
    let mut fb = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
    let block = b.create_block();
    b.append_block_params_for_function_params(block);
    b.switch_to_block(block);
    let callee = module.declare_func_in_func(target, b.func);
    let n = b.block_params(block)[0];
    let call = b.ins().call(callee, &[n]);
    let r = b.inst_results(call)[0];
    b.ins().return_(&[r]);
    b.seal_all_blocks();
    b.finalize(module.target_config());
    module.define_function(id, &mut ctx).map_err(|e| format!("{e:?}"))
}
