//! Executes NIR method bodies.

pub mod code;
mod compute;
mod dispatch;
mod exec;
mod invoke;
mod lower;
mod memory;
mod ops;
mod profile;
mod reflective;
mod shortcut;
mod unwind;
mod value;

use crate::natives::Native;
use crate::{Vm, VmError};
use code::Code;
use nir::{Defn, DefnKind, GlobalId};
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub type FuncId = u32;

/// Function references are tagged so they never collide with a real address.
const FUNC_TAG: u64 = 0xF1C0 << 48;

pub use unwind::{Flow, Unwind, Unwound};

pub enum Func {
    Nir { name: GlobalId, defn: Rc<Defn>, code: Option<Rc<Code>> },
    Native { name: GlobalId, native: Rc<Native> },
    Proxy { name: GlobalId, proxy: Rc<reflective::Proxy> },
}

#[derive(Default)]
pub struct Funcs {
    all: Vec<Func>,
    by_name: FxHashMap<GlobalId, FuncId>,
    globals: FxHashMap<GlobalId, usize>,
    consts: FxHashMap<nir::ValId, usize>,
    stack: Vec<FuncId>,
    proxies: FxHashMap<(crate::class::ClassId, nir::StrId), FuncId>,
    stack_floor: usize,
    overflowing: bool,
    pub trace: bool,
    pub profile: profile::Profile,
}

impl Funcs {
    /// Interpreted calls whose frame would lie below `floor` throw `StackOverflowError`.
    pub fn set_stack_floor(&mut self, floor: usize) {
        self.stack_floor = floor;
    }

    /// Whether calls must go through `Vm::call`, which traces and profiles.
    pub fn instrumented(&self) -> bool {
        self.trace || self.profile.on
    }
}

pub fn func_ref(id: FuncId) -> u64 {
    FUNC_TAG | id as u64
}

/// Whether `v` is a function reference only the VM can call.
pub fn is_func_ref(v: u64) -> bool {
    v & (0xFFFF << 48) == FUNC_TAG
}

impl Vm {
    /// The function defined by the method or extern `name`.
    pub fn func(&mut self, name: GlobalId) -> Result<FuncId, VmError> {
        if let Some(&id) = self.funcs.by_name.get(&name) {
            return Ok(id);
        }
        self.link_owner(name)?;
        if let Some(&id) = self.funcs.by_name.get(&name) {
            return Ok(id);
        }
        let func = match self.native(name) {
            Some(native) => Func::Native { name, native: Rc::new(native) },
            None => {
                let defn = self.world.defn(name)?;
                match &defn.kind {
                    DefnKind::Define { .. } => Func::Nir { name, defn, code: None },
                    _ => return Err(VmError::Unsupported(format!("native {}", self.world.name(name)))),
                }
            }
        };
        let id = self.funcs.all.len() as FuncId;
        self.funcs.all.push(func);
        self.funcs.by_name.insert(name, id);
        Ok(id)
    }

    pub fn call_global(&mut self, name: GlobalId, args: &[u64]) -> Flow {
        let f = self.func(name)?;
        self.call(f, args)
    }

    /// Calls `target`, a function value of type `ty`: a function reference, a trampoline, or
    /// the address of a C function.
    pub fn call_value_typed(&mut self, target: u64, ty: nir::TypeId, args: &[u64]) -> Flow {
        if is_func_ref(target) || self.callbacks.target(target as usize).is_some() {
            return self.call_value(target, args);
        }
        if target == 0 {
            return self.null_pointer();
        }
        self.call_c(target as usize, ty, args)
    }

    /// Calls the function behind a function reference or an extern address.
    pub fn call_value(&mut self, target: u64, args: &[u64]) -> Flow {
        if !is_func_ref(target) {
            return match self.callbacks.target(target as usize) {
                Some(f) => self.call(f, args),
                None => Err(VmError::Unsupported(format!("call through raw pointer {target:#x}")).into()),
            };
        }
        self.call((target & !FUNC_TAG) as FuncId, args)
    }

    pub fn call(&mut self, f: FuncId, args: &[u64]) -> Flow {
        let code = match &self.funcs.all[f as usize] {
            Func::Native { native, .. } => return native.clone().call(self, args),
            Func::Proxy { proxy, .. } => return self.call_proxy(&proxy.clone(), args),
            Func::Nir { code: Some(code), .. } => code.clone(),
            Func::Nir { defn, code: None, .. } => {
                let code = Rc::new(lower::lower(defn.clone(), &self.world.pool));
                if let Func::Nir { code: slot, .. } = &mut self.funcs.all[f as usize] {
                    *slot = Some(code.clone());
                }
                code
            }
        };
        if self.stack_exhausted() {
            return self.stack_overflow();
        }
        if !self.funcs.instrumented() {
            return match &code.shortcut {
                Some(s) => self.shortcut(&code, s, args),
                None => self.exec(&code, args),
            };
        }
        if self.funcs.profile.on {
            self.funcs.profile.count(f);
        }
        self.funcs.stack.push(f);
        let result = self.exec(&code, args);
        self.funcs.stack.pop();
        result
    }

    /// Whether the current native frame lies below the stack floor.
    #[inline]
    pub fn stack_exhausted(&self) -> bool {
        let marker = 0u8;
        (&raw const marker as usize) < self.funcs.stack_floor && !self.funcs.overflowing
    }

    /// The names of the innermost `depth` interpreted methods on the call stack, which is only
    /// recorded while tracing.
    pub fn backtrace(&self, depth: usize) -> Vec<String> {
        self.funcs.stack.iter().rev().take(depth).map(|&f| self.func_name(f)).collect()
    }

    pub fn func_name(&self, f: FuncId) -> String {
        match &self.funcs.all[f as usize] {
            Func::Nir { name, .. } | Func::Native { name, .. } | Func::Proxy { name, .. } => {
                self.world.name(*name)
            }
        }
    }
}
