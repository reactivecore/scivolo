//! Starting and ending green threads.

use super::{GreenThread, Saved, ThreadId};
use crate::interp::{Flow, StackGuard};
use crate::natives::StackWalk;
use crate::stack::{Coroutine, Frames};
use crate::{STACK_RESERVE, Vm, VmError, heap};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Address space of a thread's native stack and frame stack; pages are committed as calls
/// touch them.
pub const MAIN_STACK: usize = 1 << 30;
const THREAD_STACK: usize = 256 << 20;
const THREAD_FRAMES: usize = 256 << 20;

impl Vm {
    /// Starts a thread for `NativeThread` object `native` that runs `body`.
    pub fn start_thread(
        &mut self,
        native: usize,
        body: impl FnOnce(&mut Vm) -> Flow<()> + 'static,
    ) -> Flow<()> {
        let cell = match self.threads.cells.pop() {
            Some(c) => c,
            None => self.heap.permanent(16),
        };
        heap::write(cell, [0u64, native as u64]);
        self.spawn_thread(THREAD_STACK, cell, true, native, body)?;
        Ok(())
    }

    /// A new runnable thread that runs `body` with the thread cell `cell`.
    pub fn spawn_thread(
        &mut self,
        stack_size: usize,
        cell: usize,
        own_cell: bool,
        native: usize,
        body: impl FnOnce(&mut Vm) -> Flow<()> + 'static,
    ) -> Flow<ThreadId> {
        let co = Coroutine::new(stack_size, move |vm, yielder| {
            vm.threads.yielder = yielder;
            catch_unwind(AssertUnwindSafe(|| body(vm))).unwrap_or_else(|panic| {
                let what = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or(panic.downcast_ref::<&str>().copied());
                Err(VmError::Unsupported(format!("panic in a thread: {}", what.unwrap_or("?"))).into())
            })
        })
        .map_err(VmError::Unsupported)?;
        let (low, high) = co.bounds();
        let frames = Frames::new(if stack_size == MAIN_STACK { MAIN_STACK } else { THREAD_FRAMES })
            .map_err(VmError::Unsupported)?;
        let guard = StackGuard { floor: low + STACK_RESERVE.min((high - low) / 4), overflowing: false };
        let saved = Saved { frames, cell, guard, walk: StackWalk::default(), yielder: 0, errno: 0 };
        let thread = Some(Box::new(GreenThread { co, saved, sp: 0, own_cell }));
        let id = match self.threads.free.pop() {
            Some(id) => {
                self.threads.all[id] = thread;
                id
            }
            None => {
                self.threads.all.push(thread);
                self.threads.all.len() - 1
            }
        };
        self.threads.sched.add(id, native);
        if native != 0 {
            self.threads.by_native.insert(native, id);
        }
        self.publish_roots();
        Ok(id)
    }

    /// Forgets thread `id`, whose body has returned.
    pub fn thread_ended(&mut self, id: ThreadId) {
        self.release_monitors_of(id);
        let native = self.threads.sched.end(id);
        if self.threads.by_native.get(&native) == Some(&id) {
            self.threads.by_native.remove(&native);
        }
        self.threads.free.push(id);
    }

    /// Records that the running thread's `NativeThread` object is `native`.
    pub fn set_native_thread(&mut self, native: usize) {
        let id = self.threads.sched.current;
        if let Some(info) = self.threads.sched.info.get_mut(id) {
            info.native = native;
            self.threads.by_native.insert(native, id);
        }
    }
}
