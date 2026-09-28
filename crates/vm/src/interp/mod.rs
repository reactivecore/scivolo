//! Executes NIR method bodies.

pub mod code;
mod compute;
mod devirtualize;
mod dispatch;
mod exec;
mod fold;
mod funcs;
mod inline;
mod inlined;
mod invoke;
mod lower;
mod memory;
mod names;
mod ops;
mod profile;
mod reflective;
mod rename;
mod shortcut;
mod speculate;
mod unwind;
mod value;

use crate::{Vm, VmError};
use nir::{DefnKind, GlobalId};
use std::rc::Rc;

pub use funcs::{Func, FuncId, Funcs, StackGuard, func_ref, is_func_ref};
pub use unwind::{Flow, Unwind, Unwound};

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
                    DefnKind::Define { .. } => Func::Nir { name, defn, code: None, retired: None },
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
        self.call(funcs::func_of_ref(target), args)
    }

    /// `defn` lowered, with modules constructed by now as constants.
    pub fn lower_code(&mut self, defn: Rc<nir::Defn>) -> code::Code {
        let (modules, initializing) = (&self.modules, &self.threads.initializing);
        let constructed = |g: GlobalId| match initializing.contains_key(&g) {
            true => None,
            false => modules.get(&g).map(|&obj| obj as u64),
        };
        lower::lower(defn, &mut self.world.pool, &constructed)
    }

    /// Counts a call of `f` running `code`, lowering `f` again once it is hot.
    #[inline]
    pub fn count_call(&mut self, f: FuncId, code: &code::Code) {
        let n = code.calls.get();
        if n <= speculate::TIER_UP {
            code.calls.set(n + 1);
            if n == speculate::TIER_UP && !self.funcs.no_inline {
                self.tier_up(f);
            }
        }
    }

    pub fn call(&mut self, f: FuncId, args: &[u64]) -> Flow {
        let code = match &self.funcs.all[f as usize] {
            Func::Native { native, .. } => return native.clone().call(self, args),
            Func::Proxy { proxy, .. } => return self.call_proxy(&proxy.clone(), args),
            Func::Nir { code: Some(code), .. } => code.clone(),
            Func::Nir { defn, code: None, .. } => {
                let code = Rc::new(self.lower_code(defn.clone()));
                if let Func::Nir { code: slot, .. } = &mut self.funcs.all[f as usize] {
                    *slot = Some(code.clone());
                }
                code
            }
        };
        if self.stack_exhausted() {
            return self.stack_overflow();
        }
        self.count_call(f, &code);
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
        (&raw const marker as usize) < self.funcs.guard.floor && !self.funcs.guard.overflowing
    }
}
