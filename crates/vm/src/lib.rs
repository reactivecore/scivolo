//! An interpreter for Scala Native NIR.

mod class;
mod classpath;
mod error;
mod gcinfo;
mod heap;
mod interp;
mod layout;
mod linktime;
mod member_table;
mod natives;
mod object;
mod rtti;
mod session;
mod snapshot;
mod stack;
mod threads;
mod throwable;
mod world;

pub use class::{FieldSlot, array_names, instance_fields};
pub use classpath::{Classpath, nir_table};
pub use error::VmError;
pub use gcinfo::FIRST_ARRAY;
pub use interp::{box_method, bridge, lookup_order};
pub use layout::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE, align_of, size_of, struct_layout};
pub use linktime::{Prop, accessor_property, method_name, parse_member, property};
pub use natives::{
    Hole, MacroArg, QuoteHost, Reflected, constructible, element_names, element_position, embedded_content,
    embedded_resources, exit_immediately, is_element_method, is_load, may_build_throwable, provider_error,
    provider_names,
};
pub use rtti::{GC_RTTI_FIELDS, PRIMITIVES, interfaces, java_name};
pub use snapshot::{Target, Walk};
pub use throwable::{Frame, Thrown};
pub use world::{LoadError, World};

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
    resources: natives::Resources,
    linktime: linktime::Linktime,
    junit: Option<Vec<String>>,
    variadics: FxHashMap<GlobalId, (usize, usize)>,
    /// The message `dlerror` last returned to the program.
    dlerror: std::ffi::CString,
    scratch: Vec<usize>,
    /// The macro expansions run, and the frontend's side of the one running.
    quotes: natives::Expansions,
}

impl Vm {
    pub fn new(classpath: Classpath) -> Result<Self, VmError> {
        natives::load_runtime().map_err(VmError::Unsupported)?;
        let frames = stack::Frames::new(BOOT_FRAMES).map_err(VmError::Unsupported)?;
        let bounds =
            stack::bounds().ok_or_else(|| VmError::Unsupported("unknown thread stack bounds".into()))?;
        let mut heap = Heap::new(natives::symbol, bounds).map_err(VmError::Unsupported)?;
        let thread = heap.permanent(16, crate::heap::Blob::Words);
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
            resources: natives::Resources::default(),
            linktime: linktime::Linktime::default(),
            junit: None,
            variadics: FxHashMap::default(),
            dlerror: std::ffi::CString::default(),
            scratch: Vec::new(),
            quotes: natives::Expansions::default(),
        };
        vm.funcs.trace = std::env::var_os("SCIVOLO_TRACE").is_some();
        vm.funcs.no_inline = std::env::var_os("SCIVOLO_NO_INLINE").is_some();
        vm.funcs.profile.on = std::env::var_os("SCIVOLO_PROFILE").is_some();
        vm.start_threads(bounds);
        let ptr: *mut Vm = &mut vm;
        let running = vm.callbacks.enter(ptr);
        let booted = vm.bootstrap();
        drop(running);
        booted.map(|()| vm)
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
