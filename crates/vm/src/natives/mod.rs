//! Functions implemented outside NIR: C externs, link-time properties and VM overrides of
//! runtime methods.

mod callback;
mod ffi;
mod gc;
mod java_thread;
mod junit;
mod llvm;
mod repl;
mod resources;
mod runtime;
mod services;
mod threads;
mod unwind;

pub use callback::Callbacks;
pub use ffi::{
    Foreign, call as call_c_function, errno, exit_immediately, load_runtime, set_errno, symbol, take_sigint,
    watch_sigint,
};
pub use resources::Resources;
pub use threads::interrupt_thread;
pub use unwind::StackWalk;

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
    /// A new instance of the class, made with its constructor without parameters, as a provider
    /// of the service class.
    New(GlobalId, crate::class::ClassId),
}

impl Native {
    pub fn call(&self, vm: &mut Vm, args: &[u64]) -> Flow {
        match self {
            Native::Rust(f) => f(vm, args),
            Native::C(f) if f.blocking => vm.call_blocking(f, args),
            Native::C(f) => ffi::call(f, args),
            Native::Property(name) => vm.property_value(name),
            Native::New(class, service) => services::instantiate(vm, *class, *service),
        }
    }
}

impl Vm {
    /// Calls the C function at `addr`, whose NIR function type is `ty`.
    pub fn call_c(&mut self, addr: usize, ty: nir::TypeId, args: &[u64]) -> Flow {
        match ffi::foreign(&self.world.pool, ty, addr) {
            Some(f) => ffi::call(&f, args),
            None => Err(VmError::Unsupported(format!("call of C function pointer {addr:#x}")).into()),
        }
    }

    /// Calls variadic C function `g` with `args` and their numeric kinds.
    pub fn call_variadic(&mut self, g: GlobalId, args: &[(u64, Num)], ret: Num) -> Flow {
        let (addr, named) = match self.variadics.get(&g) {
            Some(&cached) => cached,
            None => {
                let defn = self.world.defn(g)?;
                let found = match (&defn.kind, self.world.pool.member_sig(g).and_then(demangle::extern_name))
                {
                    (DefnKind::Declare { ty }, Some(name)) => match self.world.pool.ty(*ty) {
                        Type::Function(params, _) => {
                            let fixed = params
                                .iter()
                                .take_while(|t| !matches!(self.world.pool.ty(**t), Type::Vararg));
                            ffi::symbol(&name).map(|addr| (addr, fixed.count()))
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let Some(found) = found else {
                    return Err(VmError::Unsupported(format!("native {}", self.world.name(g))).into());
                };
                self.variadics.insert(g, found);
                found
            }
        };
        ffi::variadic::call(addr, named, args, ret).ok_or_else(|| {
            let kinds: Vec<String> = args.iter().map(|(_, n)| format!("{n:?}")).collect();
            let what =
                format!("variadic call of {} with ({}) -> {ret:?}", self.world.name(g), kinds.join(", "));
            VmError::Unsupported(what).into()
        })
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
