//! Functions implemented outside NIR: C externs, link-time properties and VM overrides of
//! runtime methods.

mod callback;
mod ffi;
mod gc;
mod junit;
mod llvm;
mod runtime;
mod services;

pub use callback::Callbacks;
pub use ffi::load_runtime;

use crate::interp::Flow;
use crate::interp::code::Num;
use crate::linktime;
use crate::{Vm, VmError};
use nir::{DefnKind, GlobalId, Type, demangle};

pub type NativeFn = fn(&mut Vm, &[u64]) -> Flow;

pub enum Native {
    Rust(NativeFn),
    C(ffi::Foreign),
    Property(String),
}

impl Native {
    pub fn call(&self, vm: &mut Vm, args: &[u64]) -> Flow {
        match self {
            Native::Rust(f) => f(vm, args),
            Native::C(f) => ffi::call(f, args),
            Native::Property(name) => vm.property_value(name),
        }
    }
}

impl Vm {
    /// Calls variadic C function `g` with `args` and their numeric kinds.
    pub fn call_variadic(&mut self, g: GlobalId, args: &[(u64, Num)], ret: Num) -> Flow {
        let unsupported = |vm: &Vm| VmError::Unsupported(format!("native {}", vm.world.name(g)));
        let defn = self.world.defn(g)?;
        let named = match (&defn.kind, self.world.pool.member_sig(g).and_then(demangle::extern_name)) {
            (DefnKind::Declare { ty }, Some(name)) => match self.world.pool.ty(*ty) {
                Type::Function(params, _) => {
                    let named =
                        params.iter().take_while(|t| !matches!(self.world.pool.ty(**t), Type::Vararg));
                    ffi::symbol(&name).map(|addr| (addr, named.count()))
                }
                _ => None,
            },
            _ => None,
        };
        let Some((addr, named)) = named else { return Err(unsupported(self).into()) };
        ffi::variadic::call(addr, named, args, ret).ok_or_else(|| unsupported(self).into())
    }

    /// The address of the C variable declared by `g`.
    pub fn extern_variable(&mut self, g: GlobalId) -> Result<usize, VmError> {
        let sig = self.world.pool.member_sig(g).unwrap_or_default();
        demangle::extern_name(sig)
            .and_then(|name| ffi::symbol(&name))
            .ok_or_else(|| VmError::Unsupported(format!("extern variable {}", self.world.name(g))))
    }

    /// The native implementation of `g`, if the VM provides one.
    pub fn native(&mut self, g: GlobalId) -> Option<Native> {
        let sig = self.world.pool.member_sig(g)?;
        if let Some(name) = demangle::extern_name(sig) {
            return match runtime::extern_fn(&name) {
                Some(f) => Some(Native::Rust(f)),
                None => ffi::resolve(self, g, &name).map(Native::C),
            };
        }
        let owner = self.world.pool.top_name(g);
        if let Some(prop) = linktime::method_name(sig).and_then(|m| linktime::accessor_property(owner, m)) {
            return Some(Native::Property(prop));
        }
        runtime::method(owner, sig).map(Native::Rust)
    }
}
