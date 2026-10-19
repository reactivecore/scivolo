//! `ServiceLoader.load`, which Scala Native's linker resolves: the providers of a service are the
//! classes the classpath's `META-INF/services` files list, as the VM reads and checks them.

use super::body::FnBuilder;
use crate::R;
use crate::classes::ClassId;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, Signature, Value};
use cranelift_module::{DataId, FuncId, Module};
use vm::ARRAY_HEADER;

const LOADER: &str = "java.util.ServiceLoader";
const LOADER_INIT: &str = "RL15java.lang.ClassLAL32java.util.ServiceLoader$Provider_E";
const PROVIDER: &str = "D23createIntrinsicProviderL15java.lang.ClassR_L32java.util.ServiceLoader$ProviderEo";
const ERROR: &str = "java.util.ServiceConfigurationError";

/// What a provider's function makes: an instance of its class, or a `ServiceConfigurationError`
/// with this message, thrown.
enum Make {
    New(ClassId),
    Fail(String),
}

impl FnBuilder<'_, '_> {
    /// A `ServiceLoader` of the providers of `service`, a class object.
    // Temporary until #115 (service providers): the services of all `META-INF/services` files are
    // linked, not only the ones the program loads.
    pub fn service_loader(&mut self, service: Value) -> R<Value> {
        self.null_check(service)?;
        let ptr = self.decls.ptr;
        let done = self.b.create_block();
        self.b.append_block_param(done, ptr);
        for name in self.reach.world.services() {
            let Ok(cls) = self.reach.link_top(&name) else { continue };
            let mut providers = Vec::new();
            for p in vm::provider_names(self.reach.world, &name) {
                let class = self.reach.world.top(&p);
                let linked = match vm::constructible(self.reach.world, class) {
                    true => self
                        .reach
                        .link(class)
                        .ok()
                        .filter(|&c| self.reach.classes.get(c).ancestors.contains(&cls)),
                    false => None,
                };
                providers.push(match linked {
                    Some(c) => (c, Make::New(c)),
                    None => (cls, Make::Fail(vm::provider_error(vm::java_name(&name), &p))),
                });
            }
            let rtti = self.rtti_addr(cls)?;
            let (this, next) = (self.b.create_block(), self.b.create_block());
            let is = self.b.ins().icmp(IntCC::Equal, service, rtti);
            self.b.ins().brif(is, this, &[], next, &[]);
            self.b.switch_to_block(this);
            let array = self.providers(providers)?;
            self.b.ins().jump(done, &[array.into()]);
            self.b.switch_to_block(next);
        }
        let none = self.providers(Vec::new())?;
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

    /// An array of a `ServiceLoader.Provider` of each class and function of `providers`.
    fn providers(&mut self, providers: Vec<(ClassId, Make)>) -> R<Value> {
        let ptr = self.decls.ptr;
        let array_cls = self.reach.link_top("scala.scalanative.runtime.ObjectArray")?;
        self.reach.allocate(array_cls);
        let alloc = self.decls.import(self.module, "scivolo_aot_alloc_array", &[ptr, ptr, ptr], Some(ptr))?;
        let rtti = self.rtti_addr(array_cls)?;
        let (len, stride) = (self.b.ins().iconst(ptr, providers.len() as i64), self.b.ins().iconst(ptr, 8));
        let alloc = self.module.declare_func_in_func(alloc, self.b.func);
        let call = self.b.ins().call(alloc, &[rtti, len, stride]);
        let array = self.b.inst_results(call)[0];
        let owner = self.reach.world.top(LOADER);
        let create = self.reach.world.member(owner, PROVIDER);
        let create = self.decls.func(self.module, self.reach, create)?;
        for (i, (class, make)) in providers.into_iter().enumerate() {
            let make = self.instantiator(make)?;
            let make = self.module.declare_func_in_func(make, self.b.func);
            let make = self.b.ins().func_addr(ptr, make);
            let class = self.rtti_addr(class)?;
            let create = self.module.declare_func_in_func(create, self.b.func);
            let call = self.b.ins().call(create, &[class, make]);
            let provider = self.b.inst_results(call)[0];
            let at = ARRAY_HEADER as i32 + 8 * i as i32;
            self.b.ins().store(MemFlagsData::new(), provider, array, at);
        }
        Ok(array)
    }

    /// `scivolo_aot_new.<class>()`: what `make` makes, with a constructor of its class.
    fn instantiator(&mut self, make: Make) -> R<FuncId> {
        let ptr = self.decls.ptr;
        let (cls, init_sig, message): (ClassId, &str, Option<DataId>) = match &make {
            Make::New(cls) => (*cls, "RE", None),
            Make::Fail(text) => {
                let units: Vec<u16> = text.encode_utf16().collect();
                let text = self.decls.string(self.module, self.reach, &units)?;
                (self.reach.link_top(ERROR)?, "RL16java.lang.StringE", Some(text))
            }
        };
        let name = self.reach.world.pool.mangle(self.reach.classes.get(cls).name);
        let name = match &make {
            Make::New(_) => format!("scivolo_aot_new.{name}"),
            Make::Fail(_) => format!("scivolo_aot_new.{}.{name}", self.decls.anon),
        };
        self.decls.anon += 1;
        self.reach.allocate(cls);
        let init = self.reach.world.member(self.reach.classes.get(cls).name, init_sig);
        let init = self.decls.func(self.module, self.reach, init)?;
        let alloc = self.decls.import(self.module, "scivolo_aot_alloc", &[ptr, ptr], Some(ptr))?;
        let rtti = self.decls.rtti(self.module, self.reach, cls)?;
        let size = self.reach.classes.get(cls).size as i64;
        let slot = self.decls.exception;
        let mut sig = Signature::new(self.decls.call_conv);
        sig.returns.push(AbiParam::new(ptr));
        self.decls.define_generated(self.module, &name, sig, |b, module| {
            let rtti = module.declare_data_in_func(rtti, b.func);
            let rtti = b.ins().symbol_value(ptr, rtti);
            let size = b.ins().iconst(ptr, size);
            let alloc = module.declare_func_in_func(alloc, b.func);
            let call = b.ins().call(alloc, &[rtti, size]);
            let obj = b.inst_results(call)[0];
            let init = module.declare_func_in_func(init, b.func);
            let Some(message) = message else {
                b.ins().call(init, &[obj]);
                b.ins().return_(&[obj]);
                return Ok(());
            };
            let message = module.declare_data_in_func(message, b.func);
            let message = b.ins().symbol_value(ptr, message);
            b.ins().call(init, &[obj, message]);
            let slot = module.declare_data_in_func(slot, b.func);
            let slot = b.ins().symbol_value(ptr, slot);
            b.ins().store(MemFlagsData::new(), obj, slot, 0);
            let none = b.ins().iconst(ptr, 0);
            b.ins().return_(&[none]);
            Ok(())
        })
    }
}
