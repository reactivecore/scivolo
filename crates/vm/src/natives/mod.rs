//! Functions implemented outside NIR: C externs, link-time properties and VM overrides of
//! runtime methods.

mod callback;
mod ffi;
mod llvm;
mod runtime;

pub use callback::{Callbacks, enter};
pub use ffi::load_runtime;

use crate::interp::Flow;
use crate::linktime;
use crate::{Vm, VmError};
use nir::{GlobalId, demangle};

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
