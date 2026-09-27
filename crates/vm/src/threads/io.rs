//! Blocking C calls on helper OS threads, so other green threads run meanwhile. A helper only
//! runs the C function; it never touches the VM.

use super::{ThreadId, Wait};
use crate::interp::{Flow, Unwound};
use crate::natives::{self, Foreign};
use crate::{Vm, VmError};
use rustc_hash::FxHashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

/// The outcome of one blocking call: its result or error, and the helper's `errno` after it.
type Done = (ThreadId, Result<u64, String>, i32);

pub struct Io {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    pending: usize,
    done: FxHashMap<ThreadId, (Result<u64, String>, i32)>,
}

impl Default for Io {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, pending: 0, done: FxHashMap::default() }
    }
}

impl Io {
    /// Whether a helper is still running a call.
    pub fn busy(&self) -> bool {
        self.pending > 0
    }

    /// Waits up to `timeout` (forever if `None`) for a call to finish, and returns its thread.
    pub fn wait(&mut self, timeout: Option<Duration>) -> Option<ThreadId> {
        let done = match timeout {
            Some(t) => match self.rx.recv_timeout(t) {
                Ok(d) => d,
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
            },
            None => self.rx.recv().ok()?,
        };
        Some(self.finish(done))
    }

    /// The threads whose calls have finished since the last look.
    pub fn finished(&mut self) -> Vec<ThreadId> {
        let done: Vec<Done> = self.rx.try_iter().collect();
        done.into_iter().map(|d| self.finish(d)).collect()
    }

    fn finish(&mut self, (id, result, errno): Done) -> ThreadId {
        self.pending -= 1;
        self.done.insert(id, (result, errno));
        id
    }
}

impl Vm {
    /// Calls the `blocking` C function `f`; on a helper OS thread while other threads may run.
    pub fn call_blocking(&mut self, f: &Foreign, args: &[u64]) -> Flow {
        if !self.threads.sched.others_alive() {
            return natives::call_c_function(f, args);
        }
        let id = self.threads.sched.current;
        let (f, args, tx) = (f.clone(), args.to_vec(), self.threads.sched.io.tx.clone());
        std::thread::Builder::new()
            .spawn(move || {
                let result = natives::call_c_function(&f, &args).map_err(|u| match u.into_inner() {
                    Unwound::Fatal(e) => e.to_string(),
                    Unwound::Throw(_) => "a C function threw".into(),
                });
                let _ = tx.send((id, result, natives::errno()));
            })
            .map_err(|e| VmError::Unsupported(format!("cannot start a helper thread: {e}")))?;
        self.threads.sched.io.pending += 1;
        loop {
            self.block(Wait::Io, None);
            if let Some((result, errno)) = self.threads.sched.io.done.remove(&id) {
                natives::set_errno(errno);
                return result.map_err(|e| VmError::Unsupported(format!("blocking C call: {e}")).into());
            }
        }
    }
}
