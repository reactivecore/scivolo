//! Monitors of `synchronized`, `Object.wait` and `notify`, kept beside the objects and keyed by
//! their addresses, which never change.

use super::{ThreadId, Wait};
use crate::Vm;
use crate::interp::Flow;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Default)]
pub struct Monitors(FxHashMap<usize, Monitor>);

#[derive(Default)]
struct Monitor {
    owner: Option<ThreadId>,
    count: u32,
    /// Threads blocked entering, in arrival order.
    entry: VecDeque<ThreadId>,
    /// Threads in `Object.wait`.
    waiters: VecDeque<ThreadId>,
}

impl Monitors {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Vm {
    pub fn monitor_enter(&mut self, obj: usize) -> Flow<()> {
        if obj == 0 {
            return self.null_pointer();
        }
        let me = self.threads.sched.current;
        if self.try_enter(obj, me) {
            return Ok(());
        }
        let before = self.set_thread_state("WaitingOnMonitorEnter")?;
        if !self.try_enter(obj, me) {
            self.threads.monitors.0.entry(obj).or_default().entry.push_back(me);
            while self.threads.monitors.0.get(&obj).and_then(|m| m.owner) != Some(me) {
                self.block(Wait::Monitor, None);
            }
        }
        self.put_thread_state(before)?;
        Ok(())
    }

    /// Takes or re-enters `obj`'s monitor for thread `me` unless another thread owns it.
    fn try_enter(&mut self, obj: usize, me: ThreadId) -> bool {
        let m = self.threads.monitors.0.entry(obj).or_default();
        match m.owner {
            None => {
                m.owner = Some(me);
                m.count = 1;
                true
            }
            Some(owner) if owner == me => {
                m.count += 1;
                true
            }
            Some(_) => false,
        }
    }

    pub fn monitor_exit(&mut self, obj: usize) -> Flow<()> {
        self.owned(obj)?;
        let m = self.threads.monitors.0.get_mut(&obj).expect("owned");
        m.count -= 1;
        if m.count == 0 {
            self.release(obj);
        }
        Ok(())
    }

    /// `obj.wait()` until notified, interrupted or past `deadline`.
    pub fn monitor_wait(&mut self, obj: usize, deadline: Option<Instant>) -> Flow<()> {
        self.owned(obj)?;
        let me = self.threads.sched.current;
        self.threads.sched.info[me].interrupt = false;
        if self.take_interrupt()? {
            return self.throw_new("java.lang.InterruptedException").map(|_| ());
        }
        let before =
            self.set_thread_state(if deadline.is_some() { "WaitingWithTimeout" } else { "Waiting" })?;
        let m = self.threads.monitors.0.get_mut(&obj).expect("owned");
        let count = std::mem::take(&mut m.count);
        m.waiters.push_back(me);
        self.release(obj);
        if !std::mem::take(&mut self.threads.sched.info[me].interrupt) {
            self.block(Wait::Notify, deadline);
        }
        self.put_thread_state(before)?;
        if let Some(m) = self.threads.monitors.0.get_mut(&obj) {
            m.waiters.retain(|&t| t != me);
        }
        self.monitor_enter(obj)?;
        self.threads.monitors.0.get_mut(&obj).expect("entered").count = count;
        if self.take_interrupt()? {
            return self.throw_new("java.lang.InterruptedException").map(|_| ());
        }
        Ok(())
    }

    /// Wakes one (`all == false`) or every thread waiting on `obj`.
    pub fn monitor_notify(&mut self, obj: usize, all: bool) -> Flow<()> {
        self.owned(obj)?;
        let t = &mut self.threads;
        let m = t.monitors.0.get_mut(&obj).expect("owned");
        while let Some(id) = m.waiters.pop_front() {
            if t.sched.wake_if(id, Wait::Notify) && !all {
                break;
            }
        }
        Ok(())
    }

    pub fn holds_monitor(&self, obj: usize) -> bool {
        let owner = self.threads.monitors.0.get(&obj).and_then(|m| m.owner);
        owner == Some(self.threads.sched.current)
    }

    /// Throws `IllegalMonitorStateException` unless the running thread owns `obj`'s monitor.
    fn owned(&mut self, obj: usize) -> Flow<()> {
        match obj {
            0 => self.null_pointer(),
            _ if self.holds_monitor(obj) => Ok(()),
            _ => self.throw_new("java.lang.IllegalMonitorStateException").map(|_| ()),
        }
    }

    /// Releases the monitors thread `id`, which ended, still holds.
    pub fn release_monitors_of(&mut self, id: ThreadId) {
        let monitors = &self.threads.monitors.0;
        let held: Vec<usize> =
            monitors.iter().filter(|(_, m)| m.owner == Some(id)).map(|(&o, _)| o).collect();
        for obj in held {
            self.release(obj);
        }
    }

    /// Hands `obj`'s monitor, no longer held, to the next entering thread, or frees it.
    fn release(&mut self, obj: usize) {
        let m = self.threads.monitors.0.get_mut(&obj).expect("a monitor");
        match m.entry.pop_front() {
            Some(next) => {
                m.owner = Some(next);
                m.count = 1;
                self.threads.sched.wake(next);
            }
            None if m.waiters.is_empty() => {
                self.threads.monitors.0.remove(&obj);
            }
            None => m.owner = None,
        }
    }
}
