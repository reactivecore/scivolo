//! Blocking, yielding and preemption of the running thread, and stopping the main thread on
//! SIGINT (ADR 0013).

use super::{ThreadId, Wait};
use crate::interp::{Flow, FuncId};
use crate::natives::clock;
use crate::{Vm, VmError, natives};
use std::time::Duration;

/// The longest the idle scheduler waits before looking for a SIGINT.
const SIGNAL_SLICE: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct Interrupts {
    /// Whether SIGINTs stop the main thread.
    watching: bool,
    pub main: ThreadId,
    /// SIGINTs since the last `clear_interrupts`.
    signals: u32,
    /// Whether the main thread is to throw `ThreadDeath` at its next safepoint.
    stop: bool,
    /// C callbacks the main thread runs, which an exception must not leave.
    callbacks: u32,
    /// How long a run of the main thread may take before it is stopped as by a SIGINT.
    limit: Option<Duration>,
    /// The point on `clock::monotonic` at which the current run is stopped.
    deadline: Option<Duration>,
}

impl Vm {
    /// Blocks the running thread for `why` until another thread wakes it (`true`) or
    /// `deadline` passes (`false`); returns `false` at once outside green threads.
    pub fn block(&mut self, why: Wait, deadline: Option<Duration>) -> bool {
        self.threads.sched.block_current(why, deadline);
        let id = self.threads.sched.current;
        if !self.suspend_thread() {
            self.threads.sched.info[id].blocked = None;
            return false;
        }
        self.threads.sched.info[id].woken
    }

    /// Refuses a wait without `deadline`, which no other thread could end: none runs in
    /// portable mode.
    #[cfg(portable)]
    pub fn refuse_endless_wait(&self, deadline: Option<Duration>) -> Flow<()> {
        match deadline {
            Some(_) => Ok(()),
            None => Err(VmError::Unsupported("deadlock: every thread waits forever".into()).into()),
        }
    }

    /// Lets every other runnable thread run before the running one continues.
    pub fn yield_now(&mut self) {
        if self.threads.sched.requeue_current() && !self.suspend_thread() {
            self.threads.sched.unqueue_current();
        }
    }

    /// Counts a call or loop back-edge; every `TICKS` of them, looks for a SIGINT, lets other
    /// threads run and stops the main thread if asked to.
    #[inline]
    pub fn tick(&mut self) -> Flow<()> {
        if self.threads.sched.tick() {
            return self.preempt();
        }
        Ok(())
    }

    #[cold]
    fn preempt(&mut self) -> Flow<()> {
        self.poll_signals();
        self.yield_now();
        self.safepoint()
    }

    /// Throws `ThreadDeath` in the main thread if a SIGINT asked to stop it and it runs no C
    /// callback.
    fn safepoint(&mut self) -> Flow<()> {
        let i = &mut self.threads.interrupts;
        if !i.stop || self.threads.sched.current != i.main || i.callbacks > 0 {
            return Ok(());
        }
        i.stop = false;
        self.throw_new("java.lang.ThreadDeath").map(drop)
    }

    /// Calls `f` for C; the main thread is not stopped until it returns.
    pub fn call_from_c(&mut self, f: FuncId, args: &[u64]) -> Flow {
        let main = u32::from(self.threads.sched.current == self.threads.interrupts.main);
        self.threads.interrupts.callbacks += main;
        let result = self.call(f, args);
        self.threads.interrupts.callbacks -= main;
        result
    }

    /// Takes the SIGINTs: the first asks the main thread to stop and wakes it if it sleeps,
    /// parks or waits; the second ends the process.
    pub fn poll_signals(&mut self) {
        let i = &mut self.threads.interrupts;
        if !i.watching {
            return;
        }
        let expired = i.deadline.is_some_and(|d| clock::monotonic() >= d);
        if expired {
            i.deadline = None;
        }
        let taken = natives::take_sigint() + u32::from(expired);
        if taken == 0 {
            return;
        }
        i.signals += taken;
        if i.signals > 1 {
            eprintln!("scivolo: terminating");
            natives::exit_immediately(130);
        }
        i.stop = true;
        let main = i.main;
        self.threads.sched.expire_ticks();
        natives::interrupt_thread(self, main);
    }

    /// How long the idle scheduler may wait before calling `poll_signals`.
    pub fn signal_slice(&self) -> Option<Duration> {
        self.threads.interrupts.watching.then_some(SIGNAL_SLICE)
    }

    /// Makes SIGINTs stop the main thread instead of ending the process.
    pub fn watch_interrupts(&mut self) -> Result<(), VmError> {
        natives::watch_sigint().map_err(VmError::Unsupported)?;
        self.threads.interrupts.watching = true;
        Ok(())
    }

    /// Makes every later run that `clear_interrupts` starts stop after `limit`, as on a SIGINT;
    /// takes effect once interrupts are watched.
    pub fn limit_runs(&mut self, limit: Option<Duration>) {
        self.threads.interrupts.limit = limit;
    }

    /// Forgets the SIGINTs received so far and starts the time limit of the next run.
    pub fn clear_interrupts(&mut self) {
        natives::take_sigint();
        let i = &mut self.threads.interrupts;
        i.signals = 0;
        i.stop = false;
        i.deadline = i.limit.map(|l| clock::monotonic() + l);
    }
}
