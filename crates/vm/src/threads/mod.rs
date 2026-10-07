//! Green threads (ADR 0003): each Java thread runs as a coroutine with its own native stack and
//! frame stack. The running thread's interpreter state lives in `Vm`; a switch swaps it with
//! the state saved in the thread's record.

mod io;
mod monitors;
mod preempt;
mod roots;
mod sched;
mod spawn;

pub use monitors::Monitors;
pub use sched::{Sched, Wait};

use preempt::Interrupts;
use sched::Next;

use crate::heap::ThreadRoots;
use crate::interp::{Flow, StackGuard};
use crate::natives::{self, StackWalk};
use crate::stack::{self, Coroutine, Frames};
use crate::{Vm, VmError, heap};
use rustc_hash::FxHashMap;

pub type ThreadId = usize;

#[derive(Default)]
pub struct Threads {
    all: Vec<Option<Box<GreenThread>>>,
    /// Ids of ended threads, for reuse.
    free: Vec<ThreadId>,
    /// Native stack of the OS thread the scheduler runs on.
    os_stack: (usize, usize),
    /// While a thread runs, the frame stack its record holds and the scheduler's native stack.
    running: Option<ThreadRoots>,
    /// Top of the native stack of the thread that suspended last.
    suspended_sp: usize,
    /// What `stack::suspend` needs to suspend the running thread; 0 outside green threads.
    yielder: usize,
    roots: Vec<ThreadRoots>,
    pub sched: Sched,
    pub monitors: Monitors,
    /// The thread of each `NativeThread` object.
    pub by_native: FxHashMap<usize, ThreadId>,
    /// Thread cells of ended threads, for reuse.
    cells: Vec<usize>,
    /// Modules under construction: the constructing thread and the threads waiting for it.
    pub initializing: FxHashMap<nir::GlobalId, (ThreadId, Vec<ThreadId>)>,
    interrupts: Interrupts,
}

struct GreenThread {
    co: Coroutine,
    saved: Saved,
    /// Top of the native stack while suspended, 0 before the first run.
    sp: usize,
    /// Whether the thread cell returns to `cells` when the thread ends.
    own_cell: bool,
}

/// The per-thread part of the interpreter's state.
pub struct Saved {
    pub frames: Frames,
    pub cell: usize,
    pub guard: StackGuard,
    pub walk: StackWalk,
    pub yielder: usize,
    /// C's `errno`, which green threads on one OS thread share.
    pub errno: i32,
}

impl Drop for Threads {
    /// Leaves suspended threads as they are: unwinding them would run code against a VM that is
    /// being dropped.
    fn drop(&mut self) {
        for t in self.all.drain(..).flatten() {
            std::mem::forget(t);
        }
    }
}

impl Vm {
    /// Makes the collector scan the frame stack of code running before any green thread.
    pub fn start_threads(&mut self, os_stack: (usize, usize)) {
        self.threads.os_stack = os_stack;
        self.publish_roots();
    }

    /// Runs `body` as the main thread, and every thread it starts, until the main thread ends.
    pub fn run_threads(&mut self, body: impl FnOnce(&mut Vm) -> Flow<()> + 'static) -> Flow<()> {
        let native = heap::read::<u64>(self.thread + 8) as usize;
        let main = self.spawn_thread(spawn::MAIN_STACK, self.thread, false, native, body)?;
        self.schedule(main)
    }

    /// Runs threads until thread `main` ends, and returns its result.
    fn schedule(&mut self, main: ThreadId) -> Flow<()> {
        self.threads.interrupts.main = main;
        loop {
            self.poll_signals();
            let id = match self.threads.sched.next(self.signal_slice()) {
                Next::Run(id) => id,
                Next::Waited => continue,
                Next::Deadlock => {
                    return Err(VmError::Unsupported("deadlock: every thread waits forever".into()).into());
                }
            };
            self.threads.sched.current = id;
            let Some(result) = self.resume_thread(id) else { continue };
            self.thread_ended(id);
            if id == main {
                return result;
            }
            if let Err(u) = result
                && u.thrown().is_none()
            {
                return Err(u);
            }
        }
    }

    /// Runs thread `id` until it suspends (`None`) or ends with its result.
    fn resume_thread(&mut self, id: ThreadId) -> Option<Flow<()>> {
        let mut t = self.threads.all[id].take().expect("a suspended thread");
        self.swap_saved(&mut t.saved);
        t.sp = 0;
        let scheduler_sp = stack::approximate_sp();
        let frames = t.saved.frames.bounds();
        self.threads.running = Some(ThreadRoots { frames, low: scheduler_sp, high: self.threads.os_stack.1 });
        self.publish_roots();
        self.heap.switch_stack(t.co.bounds());
        let os_errno = natives::errno();
        natives::set_errno(t.saved.errno);
        let result = t.co.resume(self);
        t.saved.errno = natives::errno();
        natives::set_errno(os_errno);
        self.heap.switch_stack(self.threads.os_stack);
        self.swap_saved(&mut t.saved);
        self.threads.running = None;
        match result {
            None => {
                t.sp = self.threads.suspended_sp;
                self.threads.all[id] = Some(t);
            }
            Some(_) if t.own_cell => self.threads.cells.push(t.saved.cell),
            Some(_) => {}
        }
        self.publish_roots();
        result
    }

    /// Suspends the running thread until the scheduler resumes it; `false` outside green threads,
    /// where nothing can be suspended.
    pub fn suspend_thread(&mut self) -> bool {
        if self.threads.yielder == 0 {
            return false;
        }
        self.threads.suspended_sp = stack::approximate_sp();
        stack::suspend(self.threads.yielder, self);
        true
    }

    fn swap_saved(&mut self, saved: &mut Saved) {
        std::mem::swap(&mut self.frames, &mut saved.frames);
        std::mem::swap(&mut self.thread, &mut saved.cell);
        std::mem::swap(&mut self.funcs.guard, &mut saved.guard);
        std::mem::swap(&mut self.stack_walk, &mut saved.walk);
        std::mem::swap(&mut self.threads.yielder, &mut saved.yielder);
    }
}
