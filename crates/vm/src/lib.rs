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

/// Whether the VM runs in portable mode (ADR 0127): without the C runtime, files and threads.
pub const PORTABLE: bool = cfg!(portable);

pub use class::{FieldSlot, array_names, instance_fields};
pub use classpath::Classpath;
pub use error::VmError;
pub use gcinfo::FIRST_ARRAY;
pub use interp::{box_method, bridge, lookup_order};
pub use layout::{ARRAY_HEADER, ARRAY_LENGTH, ARRAY_STRIDE, align_of, size_of, struct_layout};
pub use linktime::{Prop, accessor_property, method_name, parse_member, property};
pub use member_table::nir_table;
pub use natives::{
    Hole, MacroArg, QuoteHost, Reflected, Written, constructible, element_ip_index, element_names,
    element_position, embedded_content, embedded_resources, exit_immediately, is_load, may_build_throwable,
    provider_error, provider_names,
};
pub use rtti::{GC_RTTI_FIELDS, PRIMITIVES, interfaces, java_name};
pub use snapshot::{INIT_PORTED_FROM, Snapshot, Target, Walk};
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
    #[cfg(not(portable))]
    dlerror: std::ffi::CString,
    scratch: Vec<usize>,
    /// The macro expansions run, and the frontend's side of the one running.
    quotes: natives::Expansions,
    /// Where a snapshot is written once the runtime is initialized.
    snapshot_to: Option<std::path::PathBuf>,
    /// Set when the VM was restored from a snapshot: where `init`'s own effects go.
    restored: Option<snapshot::ExecInfo>,
    /// What the program wrote to stdout and stderr in portable mode, not yet taken.
    output: Vec<natives::Written>,
}

impl Vm {
    pub fn new(classpath: Classpath) -> Result<Self, VmError> {
        Self::with_snapshot(classpath, Snapshot::Cold)
    }

    /// A VM started as `snapshot` says; a snapshot that does not fit this process starts cold.
    pub fn with_snapshot(classpath: Classpath, snapshot: Snapshot) -> Result<Self, VmError> {
        natives::load_runtime().map_err(VmError::Unsupported)?;
        let frames = stack::Frames::new(BOOT_FRAMES).map_err(VmError::Unsupported)?;
        let bounds =
            stack::bounds().ok_or_else(|| VmError::Unsupported("unknown thread stack bounds".into()))?;
        let mut heap = Heap::new(natives::symbol, bounds).map_err(VmError::Unsupported)?;
        let mut classpath = classpath;
        let (mut image, snapshot_to) = match &snapshot {
            Snapshot::Cold => (None, None),
            Snapshot::Write(path) => (None, Some(path.clone())),
            Snapshot::Restore(data, replace) => match snapshot::parse(data) {
                Ok(image) if classpath.shadows(&image.packages) => (None, None),
                Ok(image) => (Some(image), replace.clone()),
                Err(_) => {
                    if let Some(path) = replace {
                        let _ = std::fs::remove_file(path);
                    }
                    (None, replace.clone())
                }
            },
        };
        image = image.filter(|i| heap.restore(i.start, i.region));
        let thread = match &image {
            Some(i) => i.thread,
            None => heap.permanent(16, heap::Blob::Words),
        };
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
            #[cfg(not(portable))]
            dlerror: std::ffi::CString::default(),
            scratch: Vec::new(),
            quotes: natives::Expansions::default(),
            snapshot_to: None,
            restored: None,
            output: Vec::new(),
        };
        vm.funcs.trace = std::env::var_os("SCIVOLO_TRACE").is_some();
        vm.funcs.no_inline = std::env::var_os("SCIVOLO_NO_INLINE").is_some();
        vm.funcs.profile.on = std::env::var_os("SCIVOLO_PROFILE").is_some();
        vm.start_threads(bounds);
        let ptr: *mut Vm = &mut vm;
        let running = vm.callbacks.enter(ptr);
        let restored = image.as_mut().map(|i| vm.restore(i));
        if let Some(Err(e)) = &restored {
            if vm.funcs.trace {
                eprintln!("{e}");
            }
            vm.forget_snapshot();
        }
        if vm.restored.is_none() {
            vm.snapshot_to = snapshot_to;
        }
        if vm.funcs.trace && matches!(snapshot, Snapshot::Restore(..)) {
            eprintln!("snapshot {}", if vm.restored.is_some() { "restored" } else { "not used" });
        }
        let booted = match vm.restored {
            Some(_) => Ok(()),
            None => vm.bootstrap(),
        };
        drop(running);
        booted.map(|()| vm)
    }

    /// Exits the process with `code` from a main green thread, on which the exit handlers C
    /// registered (`Runtime`'s shutdown hooks) run with the VM and its threads.
    #[cfg(not(portable))]
    pub fn exit(&mut self, code: i32) -> ! {
        let _ = self.run_main_thread(move |_| -> interp::Flow<()> { std::process::exit(code) });
        std::process::exit(code)
    }

    pub fn heap_bytes(&self) -> usize {
        self.heap.allocated
    }

    /// Makes `bytes` of live objects the most before `OutOfMemoryError` (portable mode).
    #[cfg(portable)]
    pub fn set_heap_cap(&mut self, bytes: usize) {
        self.heap.set_cap(bytes);
    }

    /// Bytes of objects allocated and not yet collected (portable mode).
    #[cfg(portable)]
    pub fn heap_used(&self) -> usize {
        self.heap.used()
    }
}
