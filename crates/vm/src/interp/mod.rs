//! Executes NIR method bodies.

mod dispatch;
mod exec;
mod memory;
mod ops;
mod value;

use crate::natives::Native;
use crate::{Vm, VmError};
use nir::{Defn, DefnKind, Global, GlobalId, Inst};
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub use exec::Code;

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
            return Err(VmError::Unsupported(format!("call through raw pointer {target:#x}")).into());
        }
        self.call((target & !FUNC_TAG) as FuncId, args)
    }

    pub fn call(&mut self, f: FuncId, args: &[u64]) -> Flow {
        let (defn, code) = match &self.funcs.all[f as usize] {
            Func::Native { native, .. } => return native.clone().call(self, args),
            Func::Nir { defn, code: Some(code), .. } => (defn.clone(), code.clone()),
            Func::Nir { defn, code: None, .. } => {
                let defn = defn.clone();
                let code = Rc::new(Code::new(&defn));
                if let Func::Nir { code: slot, .. } = &mut self.funcs.all[f as usize] {
                    *slot = Some(code.clone());
                }
                (defn, code)
            }
        };
        let DefnKind::Define { body, .. } = &defn.kind else { unreachable!("only defines are compiled") };
        self.exec(&body.insts, &code, args)
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

    pub fn null_pointer<T>(&mut self) -> Flow<T> {
        self.throw_runtime("D16throwNullPointernEO", &[]).map(|_| unreachable!("throws"))
    }

    pub fn is_member(&self, g: GlobalId) -> bool {
        matches!(self.world.pool.global(g), Global::Member(..))
    }
}

fn entry_params(insts: &[Inst]) -> &[nir::Param] {
    match insts.first() {
        Some(Inst::Label(_, params)) => params,
        _ => &[],
    }
}
