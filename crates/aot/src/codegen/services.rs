//! `ServiceLoader.load`, which Scala Native's linker resolves: the providers of a service are the
//! classes the classpath's `META-INF/services` files list, as the VM reads them.

use super::body::FnBuilder;
use crate::R;
use crate::classes::{ClassId, Kind};
use cranelift_codegen::Context;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, Signature, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use vm::ARRAY_HEADER;

const LOADER: &str = "java.util.ServiceLoader";
const LOADER_INIT: &str = "RL15java.lang.ClassLAL32java.util.ServiceLoader$Provider_E";
const PROVIDER: &str = "D23createIntrinsicProviderL15java.lang.ClassR_L32java.util.ServiceLoader$ProviderEo";

impl FnBuilder<'_, '_> {
    /// A `ServiceLoader` of the providers of `service`, a class object.
    // Temporary until #115 (service providers): a listed class that is not a concrete class with
    // a constructor without parameters is left out instead of failing on use.
    pub fn service_loader(&mut self, service: Value) -> R<Value> {
        let ptr = self.decls.ptr;
        let done = self.b.create_block();
        self.b.append_block_param(done, ptr);
        for name in self.reach.world.services() {
            let Ok(cls) = self.reach.link_top(&name) else { continue };
            let mut providers = Vec::new();
            for p in vm::provider_names(self.reach.world, &name) {
                let Ok(p) = self.reach.link_top(&p) else { continue };
                let c = self.reach.classes.get(p);
                let init = self.reach.world.member(c.name, "RE");
                let concrete = c.kind == Kind::Class && self.reach.world.defn(init).is_ok();
                if concrete && c.ancestors.contains(&cls) {
                    providers.push(p);
                }
            }
            let rtti = self.rtti_addr(cls)?;
            let (this, next) = (self.b.create_block(), self.b.create_block());
            let is = self.b.ins().icmp(IntCC::Equal, service, rtti);
            self.b.ins().brif(is, this, &[], next, &[]);
            self.b.switch_to_block(this);
            let array = self.providers(&providers)?;
            self.b.ins().jump(done, &[array.into()]);
            self.b.switch_to_block(next);
        }
        let none = self.providers(&[])?;
        self.b.ins().jump(done, &[none.into()]);
        self.b.switch_to_block(done);
        let array = self.b.block_params(done)[0];
        let cls = self.reach.link_top(LOADER)?;
        let loader = self.alloc(cls)?;
        let init = self.reach.world.member(self.reach.classes.get(cls).name, LOADER_INIT);
        let init = self.decls.func(self.module, self.reach, init)?;
        let init = self.module.declare_func_in_func(init, self.b.func);
        self.b.ins().call(init, &[loader, service, array]);
        Ok(loader)
    }

    /// An array of a `ServiceLoader.Provider` of each class of `providers`.
    fn providers(&mut self, providers: &[ClassId]) -> R<Value> {
        let ptr = self.decls.ptr;
        let array_cls = self.reach.link_top("scala.scalanative.runtime.ObjectArray")?;
        self.reach.allocate(array_cls);
        let alloc = self.decls.import(self.module, "scalai_aot_alloc_array", &[ptr, ptr, ptr], Some(ptr))?;
        let rtti = self.rtti_addr(array_cls)?;
        let (len, stride) = (self.b.ins().iconst(ptr, providers.len() as i64), self.b.ins().iconst(ptr, 8));
        let alloc = self.module.declare_func_in_func(alloc, self.b.func);
        let call = self.b.ins().call(alloc, &[rtti, len, stride]);
        let array = self.b.inst_results(call)[0];
        let owner = self.reach.world.top(LOADER);
        let create = self.reach.world.member(owner, PROVIDER);
        let create = self.decls.func(self.module, self.reach, create)?;
        for (i, &p) in providers.iter().enumerate() {
            let make = self.instantiator(p)?;
            let make = self.module.declare_func_in_func(make, self.b.func);
            let make = self.b.ins().func_addr(ptr, make);
            let class = self.rtti_addr(p)?;
            let create = self.module.declare_func_in_func(create, self.b.func);
            let call = self.b.ins().call(create, &[class, make]);
            let provider = self.b.inst_results(call)[0];
            let at = ARRAY_HEADER as i32 + 8 * i as i32;
            self.b.ins().store(MemFlagsData::new(), provider, array, at);
        }
        Ok(array)
    }

    /// `scalai_aot_new.<class>()`: a new instance of `cls` made with its constructor without
    /// parameters.
    fn instantiator(&mut self, cls: ClassId) -> R<FuncId> {
        let ptr = self.decls.ptr;
        let mut sig = Signature::new(self.decls.call_conv);
        sig.returns.push(AbiParam::new(ptr));
        let name =
            format!("scalai_aot_new.{}", self.reach.world.pool.mangle(self.reach.classes.get(cls).name));
        let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| e.to_string())?;
        if !self.decls.generated.insert(name.clone()) {
            return Ok(id);
        }
        self.reach.allocate(cls);
        let init = self.reach.world.member(self.reach.classes.get(cls).name, "RE");
        let init = self.decls.func(self.module, self.reach, init)?;
        let alloc = self.decls.import(self.module, "scalai_aot_alloc", &[ptr, ptr], Some(ptr))?;
        let rtti = self.decls.rtti(self.module, self.reach, cls)?;
        let mut ctx = Context::new();
        ctx.func.signature = sig;
        let mut fb = FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
        let block = b.create_block();
        b.switch_to_block(block);
        let rtti = self.module.declare_data_in_func(rtti, b.func);
        let rtti = b.ins().symbol_value(ptr, rtti);
        let size = b.ins().iconst(ptr, self.reach.classes.get(cls).size as i64);
        let alloc = self.module.declare_func_in_func(alloc, b.func);
        let call = b.ins().call(alloc, &[rtti, size]);
        let obj = b.inst_results(call)[0];
        let init = self.module.declare_func_in_func(init, b.func);
        b.ins().call(init, &[obj]);
        b.ins().return_(&[obj]);
        b.seal_all_blocks();
        b.finalize(self.module.target_config());
        self.module.define_function(id, &mut ctx).map_err(|e| format!("{name}: {e:?}"))?;
        Ok(id)
    }
}
