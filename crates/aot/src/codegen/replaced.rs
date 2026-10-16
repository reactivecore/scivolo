//! Library methods the VM replaces natively: by a C function (`Class.is`), or by a body the
//! codegen writes (the stack trace element of a frame, the linked classes `Class.forName`
//! searches, the `ServiceLoader` of a service's providers).

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::InstBuilder;
use cranelift_module::Module;
use nir::{GlobalId, Pool};

pub enum Replaced {
    Element,
    AllClasses,
    /// `ServiceLoader.load`, whose service class is parameter `service`.
    Services {
        service: usize,
    },
}

/// The C function that replaces library method `g`.
pub fn c_replacement(pool: &Pool, g: GlobalId) -> Option<&'static str> {
    const CLASS_IS: &str = "D2isL15java.lang.ClassL15java.lang.ClasszEPT15java.lang.Class";
    match (pool.top_name(g), pool.member_sig(g)?) {
        ("java.lang.Class", CLASS_IS) => Some("scalai_aot_class_is"),
        _ => None,
    }
}

/// The body the codegen writes for library method `g`.
pub fn replaced(pool: &Pool, g: GlobalId) -> Option<Replaced> {
    let (owner, sig) = (pool.top_name(g), pool.member_sig(g)?);
    match owner {
        _ if vm::is_element_method(owner, sig) => Some(Replaced::Element),
        "scala.scalanative.runtime.LinkedClassesRepository$" if sig.starts_with("D7loadAll") => {
            Some(Replaced::AllClasses)
        }
        "java.util.ServiceLoader$" if vm::is_load(sig, 'O') => Some(Replaced::Services { service: 1 }),
        "java.util.ServiceLoader" if vm::is_load(sig, 'o') => Some(Replaced::Services { service: 0 }),
        _ => None,
    }
}

impl FnBuilder<'_, '_> {
    pub fn replaced_body(&mut self, replaced: Replaced) -> R<()> {
        let block = self.b.create_block();
        self.b.append_block_params_for_function_params(block);
        self.b.switch_to_block(block);
        let params = self.b.block_params(block).to_vec();
        let result = match replaced {
            Replaced::Element => self.trace_element(params[1])?,
            Replaced::AllClasses => {
                let gv = self.module.declare_data_in_func(self.decls.all_classes, self.b.func);
                self.b.ins().symbol_value(self.decls.ptr, gv)
            }
            Replaced::Services { service } => self.service_loader(params[service])?,
        };
        self.b.ins().return_(&[result]);
        Ok(())
    }
}
