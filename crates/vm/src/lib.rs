//! An interpreter for Scala Native NIR.

mod class;
mod classpath;
mod error;
mod heap;
mod interp;
mod layout;
mod linktime;
mod natives;
mod object;
mod rtti;
mod world;

pub use classpath::Classpath;
pub use error::VmError;
pub use world::World;

use class::Classes;
use heap::Heap;
use interp::Funcs;
use nir::{GlobalId, StrId};
use rustc_hash::FxHashMap;

pub struct Vm {
    pub world: World,
    heap: Heap,
    classes: Classes,
    rtti: rtti::Rtti,
    funcs: Funcs,
    modules: FxHashMap<GlobalId, usize>,
    strings: FxHashMap<StrId, usize>,
    thread: CurrentThread,
    callbacks: natives::Callbacks,
}

/// The `java.lang.Thread` and native thread objects of the only thread.
#[derive(Default)]
struct CurrentThread {
    java: u64,
    native: u64,
}

impl Vm {
    pub fn new(classpath: Classpath) -> Result<Self, VmError> {
        natives::load_runtime().map_err(VmError::Unsupported)?;
        let mut vm = Self {
            world: World::new(classpath),
            heap: Heap::default(),
            classes: Classes::default(),
            rtti: rtti::Rtti::default(),
            funcs: Funcs::default(),
            modules: FxHashMap::default(),
            strings: FxHashMap::default(),
            thread: CurrentThread::default(),
            callbacks: natives::Callbacks::default(),
        };
        vm.funcs.trace = std::env::var_os("SCALAI_TRACE").is_some();
        vm.bootstrap()?;
        Ok(vm)
    }

    /// Runs `main(args)` of the top-level object or class `entry`.
    pub fn run_main(&mut self, entry: &str, args: &[String]) -> Result<(), VmError> {
        natives::enter(self);
        let arr = self.string_array(args)?;
        let top = self.world.top(entry);
        let main = self.world.member(top, "D4mainLAL16java.lang.String_uEo");
        match self.call_global(main, &[arr as u64]) {
            Ok(_) => Ok(()),
            Err(interp::Unwind::Throw(exc)) => Err(VmError::Uncaught(self.describe_throwable(exc))),
            Err(interp::Unwind::Fatal(e)) => Err(e),
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.heap.allocated
    }
}
