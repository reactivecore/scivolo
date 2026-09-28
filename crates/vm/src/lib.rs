//! An interpreter for Scala Native NIR.

mod class;
mod classpath;
mod error;
mod gcinfo;
mod heap;
mod image;
mod interp;
mod layout;
mod linktime;
mod natives;
mod object;
mod rtti;
mod stack;
mod threads;
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

/// Address space of the frame stack code running before any green thread uses.
const BOOT_FRAMES: usize = 256 << 20;

pub struct Vm {
    pub world: World,
    heap: Heap,
    /// The running thread's frame stack; see `threads` for the rest of its state.
    frames: stack::Frames,
    threads: threads::Threads,
    classes: Classes,
    rtti: rtti::Rtti,
    gc_classes: gcinfo::GcClasses,
    funcs: Funcs,
    modules: FxHashMap<GlobalId, usize>,
    strings: FxHashMap<StrId, usize>,
    /// A permanent cell holding the `java.lang.Thread` and native thread objects of the running
    /// thread.
    thread: usize,
    callbacks: natives::Callbacks,
    stack_walk: natives::StackWalk,
    linktime: linktime::Linktime,
    junit: Option<Vec<String>>,
    variadics: FxHashMap<GlobalId, (usize, usize)>,
    /// The message `dlerror` last returned to the program.
    dlerror: std::ffi::CString,
    scratch: Vec<usize>,
}

impl Vm {
    pub fn new(classpath: Classpath) -> Result<Self, VmError> {
        natives::load_runtime().map_err(VmError::Unsupported)?;
        let frames = stack::Frames::new(BOOT_FRAMES).map_err(VmError::Unsupported)?;
        let bounds =
            stack::bounds().ok_or_else(|| VmError::Unsupported("unknown thread stack bounds".into()))?;
        let mut heap = Heap::new(natives::symbol, bounds).map_err(VmError::Unsupported)?;
        let thread = heap.permanent(16);
        let mut vm = Self {
            world: World::new(classpath),
            heap,
            frames,
            threads: threads::Threads::default(),
            classes: Classes::default(),
            rtti: rtti::Rtti::default(),
            gc_classes: gcinfo::GcClasses::default(),
            funcs: Funcs::default(),
            modules: FxHashMap::default(),
            strings: FxHashMap::default(),
            thread,
            callbacks: natives::Callbacks::default(),
            stack_walk: natives::StackWalk::default(),
            linktime: linktime::Linktime::default(),
            junit: None,
            variadics: FxHashMap::default(),
            dlerror: std::ffi::CString::default(),
            scratch: Vec::new(),
        };
        vm.funcs.trace = std::env::var_os("SCALAI_TRACE").is_some();
        vm.funcs.no_inline = std::env::var_os("SCALAI_NO_INLINE").is_some();
        vm.funcs.profile.on = std::env::var_os("SCALAI_PROFILE").is_some();
        vm.start_threads(bounds);
        let ptr: *mut Vm = &mut vm;
        let running = vm.callbacks.enter(ptr);
        let booted = vm.bootstrap();
        drop(running);
        booted.map(|()| vm)
    }

    /// Runs `main(args)` of the top-level object or class `entry` on the main green thread as a
    /// Scala Native executable does: runtime initialization, `main`, then the runtime's
    /// shutdown, which runs queued tasks. Interpreted calls throw `StackOverflowError` within
    /// `STACK_RESERVE` of the end of their thread's stack.
    pub fn run_main(&mut self, entry: &str, args: &[String]) -> Result<(), VmError> {
        let vm: *mut Vm = self;
        let _running = self.callbacks.enter(vm);
        let argv = self.c_argv(entry, args);
        let (entry, argc) = (entry.to_owned(), args.len() + 1);
        let main = move |vm: &mut Vm| {
            vm.run_program(&entry, argc, argv).map_err(|u| match u.thrown() {
                Some(exc) => VmError::Uncaught(vm.describe_throwable(exc)).into(),
                None => u,
            })
        };
        match self.run_threads(main) {
            Ok(()) => Ok(()),
            Err(u) => match u.into_inner() {
                interp::Unwound::Throw(exc) => Err(VmError::Uncaught(self.describe_throwable(exc))),
                interp::Unwound::Fatal(e) => Err(e),
            },
        }
    }

    fn run_program(&mut self, entry: &str, argc: usize, argv: usize) -> interp::Flow<()> {
        let runtime = self.world.top("scala.scalanative.runtime.package$");
        let module = self.module(runtime)? as u64;
        let init = self.world.member(runtime, "D4initiR_LAL16java.lang.String_EO");
        let args = self.call_global(init, &[module, argc as u64, argv as u64])?;
        let top = self.world.top(entry);
        let main = self.world.member(top, "D4mainLAL16java.lang.String_uEo");
        self.call_global(main, &[args])?;
        let shutdown = self.world.member(runtime, "D10onShutdownuEO");
        self.call_global(shutdown, &[module])?;
        Ok(())
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
