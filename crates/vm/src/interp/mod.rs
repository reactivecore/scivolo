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

/// Why a call did not return normally; boxed so `Flow` stays two words.
pub struct Unwind(Box<Unwound>);

pub enum Unwound {
    Throw(usize),
    Fatal(VmError),
}

impl Unwind {
    pub fn throw(exc: usize) -> Self {
        Unwind(Box::new(Unwound::Throw(exc)))
    }

    /// The exception object, unless this is a fatal error.
    pub fn thrown(&self) -> Option<usize> {
        match *self.0 {
            Unwound::Throw(exc) => Some(exc),
            Unwound::Fatal(_) => None,
        }
    }

    pub fn into_inner(self) -> Unwound {
        *self.0
    }
}

impl From<VmError> for Unwind {
    fn from(e: VmError) -> Self {
        Unwind(Box::new(Unwound::Fatal(e)))
    }
}

impl From<crate::world::LoadError> for Unwind {
    fn from(e: crate::world::LoadError) -> Self {
        VmError::from(e).into()
    }
}

pub type Flow<T = u64> = Result<T, Unwind>;

pub enum Func {
    Nir { name: GlobalId, defn: Rc<Defn>, code: Option<Rc<Code>> },
    Native { name: GlobalId, native: Rc<Native> },
}

#[derive(Default)]
pub struct Funcs {
    all: Vec<Func>,
    by_name: FxHashMap<GlobalId, FuncId>,
    globals: FxHashMap<GlobalId, usize>,
    consts: FxHashMap<nir::ValId, usize>,
    stack: Vec<FuncId>,
    frames: Vec<Vec<u64>>,
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

    /// Keeps `frame` for reuse unless enough frames are pooled already.
    pub fn recycle(&mut self, frame: Vec<u64>) {
        if self.frames.len() < MAX_POOLED_FRAMES {
            self.frames.push(frame);
        }
    }
}

/// Frames kept for reuse; deeper recursion allocates its extra frames afresh.
const MAX_POOLED_FRAMES: usize = 256;

pub fn func_ref(id: FuncId) -> u64 {
    FUNC_TAG | id as u64
}

impl Vm {
    /// The function defined by the method or extern `name`.
    pub fn func(&mut self, name: GlobalId) -> Result<FuncId, VmError> {
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

    /// Calls the function behind a function reference or an extern address.
    pub fn call_value(&mut self, target: u64, args: &[u64]) -> Flow {
        if target & FUNC_TAG != FUNC_TAG {
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
            return self.exec(&code, args);
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

    pub fn stack_overflow(&mut self) -> Flow {
        self.funcs.overflowing = true;
        let result = self.throw_new("java.lang.StackOverflowError");
        self.funcs.overflowing = false;
        result
    }

    /// The names of the innermost `depth` interpreted methods on the call stack, which is only
    /// recorded while tracing.
    pub fn backtrace(&self, depth: usize) -> Vec<String> {
        self.funcs.stack.iter().rev().take(depth).map(|&f| self.func_name(f)).collect()
    }

    pub fn func_name(&self, f: FuncId) -> String {
        match &self.funcs.all[f as usize] {
            Func::Nir { name, .. } | Func::Native { name, .. } => self.world.name(*name),
        }
    }

    /// The runtime helper `scala.scalanative.runtime.package$::<sig>` raising its exception.
    pub fn throw_runtime(&mut self, sig: &str, args: &[u64]) -> Flow {
        let owner = self.world.top("scala.scalanative.runtime.package$");
        let module = self.module(owner)?;
        let g = self.world.member(owner, sig);
        let mut all = vec![module as u64];
        all.extend_from_slice(args);
        self.call_global(g, &all)?;
        Err(VmError::Unsupported(format!("{sig} returned")).into())
    }

    /// Throws a new instance of `class`, built by its no-argument constructor.
    pub fn throw_new(&mut self, class: &str) -> Flow {
        let cls = self.link_top(class)?;
        let obj = self.alloc_object(cls);
        let init = self.world.member(self.class(cls).name, "RE");
        self.call_global(init, &[obj as u64])?;
        Err(Unwind::throw(obj))
    }

    pub fn null_pointer<T>(&mut self) -> Flow<T> {
        if self.funcs.trace {
            eprintln!("null pointer in:\n  {}", self.backtrace(12).join("\n  "));
        }
        self.throw_runtime("D16throwNullPointernEO", &[]).map(|_| unreachable!("throws"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn flow_fits_two_registers() {
        assert_eq!(size_of::<super::Flow>(), 16);
    }
}
