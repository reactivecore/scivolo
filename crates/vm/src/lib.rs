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
mod stack;
mod world;

pub use classpath::Classpath;
pub use error::VmError;
pub use natives::exit_immediately;
pub use world::World;

use class::Classes;
use heap::Heap;
use interp::Funcs;
use nir::{GlobalId, StrId};
use rustc_hash::FxHashMap;

/// Native stack kept free below the innermost interpreted call, for Rust and C code and for
/// constructing the `StackOverflowError`; at most a quarter of the stack left.
const STACK_RESERVE: usize = 4 << 20;

/// Native stack interpreted calls may use where the thread's stack bounds are unknown.
const FALLBACK_STACK_BUDGET: usize = 6 << 20;

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
    linktime: linktime::Linktime,
    junit: Option<Vec<String>>,
    variadics: FxHashMap<GlobalId, (usize, usize)>,
    scratch: Vec<usize>,
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
            linktime: linktime::Linktime::default(),
            junit: None,
            variadics: FxHashMap::default(),
            scratch: Vec::new(),
        };
        vm.funcs.trace = std::env::var_os("SCALAI_TRACE").is_some();
        vm.funcs.profile.on = std::env::var_os("SCALAI_PROFILE").is_some();
        let ptr: *mut Vm = &mut vm;
        let running = vm.callbacks.enter(ptr);
        let booted = vm.bootstrap();
        drop(running);
        booted.map(|()| vm)
    }

    /// Runs `main(args)` of the top-level object or class `entry`. Interpreted calls throw
    /// `StackOverflowError` within `STACK_RESERVE` of the end of the thread's stack.
    pub fn run_main(&mut self, entry: &str, args: &[String]) -> Result<(), VmError> {
        let here = &raw const self as usize;
        let floor = match stack::low_end() {
            Some(low) => low + STACK_RESERVE.min(here.saturating_sub(low) / 4),
            None => here.saturating_sub(FALLBACK_STACK_BUDGET),
        };
        self.funcs.set_stack_floor(floor);
        let vm: *mut Vm = self;
        let _running = self.callbacks.enter(vm);
        let arr = self.string_array(args)?;
        let top = self.world.top(entry);
        let main = self.world.member(top, "D4mainLAL16java.lang.String_uEo");
        match self.call_global(main, &[arr as u64]) {
            Ok(_) => Ok(()),
            Err(u) => match u.into_inner() {
                interp::Unwound::Throw(exc) => Err(VmError::Uncaught(self.describe_throwable(exc))),
                interp::Unwound::Fatal(e) => Err(e),
            },
        }
    }

    /// Exits the process with `code`, keeping the VM available to exit handlers C registered.
    pub fn exit(&mut self, code: i32) -> ! {
        let vm: *mut Vm = self;
        let _running = self.callbacks.enter(vm);
        std::process::exit(code)
    }

    pub fn heap_bytes(&self) -> usize {
        self.heap.allocated
    }
}
