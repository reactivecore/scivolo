//! Executes NIR method bodies.

pub mod code;
mod compute;
mod dispatch;
mod exec;
mod lower;
mod memory;
mod ops;
mod value;

use crate::natives::Native;
use crate::{Vm, VmError};
use code::Code;
use nir::{Defn, DefnKind, GlobalId, TypeId};
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub type FuncId = u32;

/// Function references are tagged so they never collide with a real address.
const FUNC_TAG: u64 = 0xF1C0 << 48;

pub enum Unwind {
    Throw(usize),
    Fatal(VmError),
}

impl From<VmError> for Unwind {
    fn from(e: VmError) -> Self {
        Unwind::Fatal(e)
    }
}

impl From<crate::world::LoadError> for Unwind {
    fn from(e: crate::world::LoadError) -> Self {
        Unwind::Fatal(e.into())
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
    boxers: FxHashMap<(TypeId, bool), (u64, FuncId)>,
    stack_floor: usize,
    overflowing: bool,
    pub trace: bool,
}

impl Funcs {
    pub fn has_stack_budget(&self) -> bool {
        self.stack_floor != 0
    }
}

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
        let marker = 0u8;
        if (&raw const marker as usize) < self.funcs.stack_floor && !self.funcs.overflowing {
            self.funcs.overflowing = true;
            let result = self.throw_new("java.lang.StackOverflowError");
            self.funcs.overflowing = false;
            return result;
        }
        self.funcs.stack.push(f);
        let result = self.exec(&code, args);
        self.funcs.stack.pop();
        result
    }

    /// The names of the innermost `depth` interpreted methods on the call stack.
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
        Err(Unwind::Throw(obj))
    }

    /// Lets interpreted calls use the native stack down to `budget` bytes below the caller's
    /// frame; deeper calls throw `StackOverflowError`.
    pub fn set_stack_budget(&mut self, budget: usize) {
        let marker = 0u8;
        self.funcs.stack_floor = (&raw const marker as usize).saturating_sub(budget);
    }

    pub fn null_pointer<T>(&mut self) -> Flow<T> {
        if self.funcs.trace {
            eprintln!("null pointer in:\n  {}", self.backtrace(12).join("\n  "));
        }
        self.throw_runtime("D16throwNullPointernEO", &[]).map(|_| unreachable!("throws"))
    }
}
