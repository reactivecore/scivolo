//! The table of functions the interpreter calls, with the caches built around it.

use super::code::{Code, Mem};
use super::{profile, reflective};
use crate::natives::Native;
use nir::{Defn, GlobalId};
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub type FuncId = u32;

/// Function references are tagged so they never collide with a real address.
const FUNC_TAG: u64 = 0xF1C0 << 48;

pub enum Func {
    /// A NIR method; `retired` is the code `code` replaced at tier-up, which running calls and
    /// their stack traces may still use.
    Nir {
        name: GlobalId,
        defn: Rc<Defn>,
        code: Option<Rc<Code>>,
        retired: Option<Rc<Code>>,
    },
    Native {
        name: GlobalId,
        native: Rc<Native>,
    },
    Proxy {
        name: GlobalId,
        proxy: Rc<reflective::Proxy>,
    },
}

#[derive(Default)]
pub struct Funcs {
    pub all: Vec<Func>,
    pub by_name: FxHashMap<GlobalId, FuncId>,
    pub globals: FxHashMap<GlobalId, usize>,
    pub consts: FxHashMap<nir::ValId, usize>,
    pub stack: Vec<FuncId>,
    pub proxies: FxHashMap<(crate::class::ClassId, nir::StrId), FuncId>,
    /// The running thread's native stack limit.
    pub guard: StackGuard,
    pub trace: bool,
    /// Lower methods as they are, without inlining callees.
    pub no_inline: bool,
    /// Whether each method judged so far may be inlined, with its definition if so.
    pub inlinable: FxHashMap<GlobalId, Option<Rc<Defn>>>,
    /// Per type object, the value field of a `java.lang` box as `==` compares it, if one.
    pub boxes: FxHashMap<u64, Option<(usize, Mem, bool)>>,
    pub profile: profile::Profile,
}

/// Interpreted calls whose native frame would lie below `floor` throw `StackOverflowError`,
/// unless one is being constructed (`overflowing`).
#[derive(Default)]
pub struct StackGuard {
    pub floor: usize,
    pub overflowing: bool,
}

impl Funcs {
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

/// The id of the function behind function reference `v`.
pub fn func_of_ref(v: u64) -> FuncId {
    (v & !FUNC_TAG) as FuncId
}
