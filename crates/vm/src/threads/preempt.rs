//! Blocking, yielding and preemption of the running thread, and stopping the main thread on
//! SIGINT (ADR 0013).

use super::{ThreadId, Wait};
use crate::interp::Flow;
use crate::{Vm, VmError, natives};
use std::cmp::Reverse;
use std::time::{Duration, Instant};

/// Calls and loop back-edges between checks whether another thread should run.
const TICKS: u32 = 20_000;

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
}

impl Vm {
    /// Blocks the running thread for `why` until another thread wakes it (`true`) or
    /// `deadline` passes (`false`); returns `false` at once outside green threads.
    pub fn block(&mut self, why: Wait, deadline: Option<Instant>) -> bool {
        let s = &mut self.threads.sched;
        let id = s.current;
        let info = &mut s.info[id];
        info.blocked = Some(why);
        info.woken = false;
        info.generation += 1;
        let generation = info.generation;
        if !self.suspend_after(deadline.map(|d| (d, generation))) {
            self.threads.sched.info[id].blocked = None;
            return false;
        }
        self.threads.sched.info[id].woken
    }

    fn suspend_after(&mut self, timer: Option<(Instant, u64)>) -> bool {
        if let Some((deadline, generation)) = timer {
            let s = &mut self.threads.sched;
            s.timers.push(Reverse((deadline, s.current, generation)));
        }
        self.suspend_thread()
    }

    /// Lets every other runnable thread run before the running one continues.
    pub fn yield_now(&mut self) {
        let s = &mut self.threads.sched;
        s.poll();
        if !s.queue.is_empty() {
            s.queue.push_back(s.current);
            if !self.suspend_thread() {
                self.threads.sched.queue.pop_back();
            }
        }
    }

    /// Counts a call or loop back-edge; every `TICKS` of them, looks for a SIGINT, lets other
    /// threads run and stops the main thread if asked to.
    #[inline]
    pub fn tick(&mut self) -> Flow<()> {
        let s = &mut self.threads.sched;
        s.ticks = s.ticks.wrapping_sub(1);
        if s.ticks > TICKS {
            return self.preempt();
        }
        Ok(())
    }

    #[cold]
    fn preempt(&mut self) -> Flow<()> {
        self.threads.sched.ticks = TICKS;
        self.poll_signals();
        self.yield_now();
        self.safepoint()
    }

    /// Throws `ThreadDeath` in the main thread if a SIGINT asked to stop it.
    fn safepoint(&mut self) -> Flow<()> {
        let i = &mut self.threads.interrupts;
        if !i.stop || self.threads.sched.current != i.main {
            return Ok(());
        }
        i.stop = false;
        self.throw_new("java.lang.ThreadDeath").map(drop)
    }

    /// Takes a SIGINT: the first asks the main thread to stop and wakes it if it sleeps, parks
    /// or waits; the second ends the process.
    pub fn poll_signals(&mut self) {
        let i = &mut self.threads.interrupts;
        if !i.watching || !natives::take_sigint() {
            return;
        }
        i.signals += 1;
        if i.signals > 1 {
            eprintln!("scalai: terminating");
            natives::exit_immediately(130);
        }
        i.stop = true;
        let main = i.main;
        self.threads.sched.ticks = 0;
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

    /// Forgets the SIGINTs received so far.
    pub fn clear_interrupts(&mut self) {
        natives::take_sigint();
        let i = &mut self.threads.interrupts;
        i.signals = 0;
        i.stop = false;
    }
}
