//! Module load functions, which return a module's instance and construct it on first use, and
//! the slots holding the instances: `scalai_aot_modules`, `scalai_aot_modules_count` of them.

use super::Decls;
use crate::R;
use crate::reach::Reach;
use cranelift_codegen::Context;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;

pub fn define_loads(module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls) -> R<()> {
    let ptr = decls.ptr;
    let alloc = decls.import(module, "scalai_aot_alloc", &[ptr, ptr], Some(ptr))?;
    let mut ctx = Context::new();
    let mut fb = FunctionBuilderContext::new();
    for slot in 0..reach.modules.len() {
        let cls = reach.modules[slot];
        let load = decls.module_load(module, reach, cls)?;
        let ctor = reach.constructor(cls).map(|c| decls.func(module, reach, c)).transpose()?;
        let rtti = decls.rtti(module, reach, cls)?;
        ctx.func.signature = module.declarations().get_function_decl(load).signature.clone();
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
        let (entry, init, done) = (b.create_block(), b.create_block(), b.create_block());
        b.append_block_param(done, ptr);
        b.switch_to_block(entry);
        let slots = module.declare_data_in_func(decls.module_slots, b.func);
        let slots = b.ins().symbol_value(ptr, slots);
        let instance = b.ins().load(ptr, MemFlagsData::new(), slots, 8 * slot as i32);
        b.ins().brif(instance, done, &[instance.into()], init, &[]);
        b.switch_to_block(init);
        let rtti = module.declare_data_in_func(rtti, b.func);
        let rtti = b.ins().symbol_value(ptr, rtti);
        let size = b.ins().iconst(ptr, reach.classes.get(cls).size as i64);
        let alloc = module.declare_func_in_func(alloc, b.func);
        let call = b.ins().call(alloc, &[rtti, size]);
        let instance = b.inst_results(call)[0];
        b.ins().store(MemFlagsData::new(), instance, slots, 8 * slot as i32);
        if let Some(ctor) = ctor {
            let ctor = module.declare_func_in_func(ctor, b.func);
            b.ins().call(ctor, &[instance]);
        }
        b.ins().jump(done, &[instance.into()]);
        b.switch_to_block(done);
        let instance = b.block_params(done)[0];
        b.ins().return_(&[instance]);
        b.seal_all_blocks();
        b.finalize(module.target_config());
        module.define_function(load, &mut ctx).map_err(|e| format!("{e:?}"))?;
        module.clear_context(&mut ctx);
    }
    let count = reach.modules.len();
    let mut slots = DataDescription::new();
    slots.set_align(8);
    slots.define_zeroinit(8 * count.max(1));
    module.define_data(decls.module_slots, &slots).map_err(|e| e.to_string())?;
    let id = module
        .declare_data("scalai_aot_modules_count", Linkage::Export, false, false)
        .map_err(|e| e.to_string())?;
    let mut desc = DataDescription::new();
    desc.set_align(8);
    desc.define(Box::new((count as u64).to_le_bytes()));
    module.define_data(id, &desc).map_err(|e| e.to_string())
}
