//! The scheduler: a run queue of green threads, timers, and blocking, waking and preemption.

use super::ThreadId;
use super::io::Io;
use crate::Vm;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::time::Instant;

/// Calls and loop back-edges between checks whether another thread should run.
const TICKS: u32 = 20_000;

/// What a blocked thread waits for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wait {
    Park,
    Sleep,
    /// Entry to a monitor another thread owns, or a module another thread initializes.
    Monitor,
    /// `Object.wait` on a monitor.
    Notify,
    /// A blocking C call on a helper thread.
    Io,
}

#[derive(Default)]
pub struct Sched {
    queue: VecDeque<ThreadId>,
    timers: BinaryHeap<Reverse<(Instant, ThreadId, u64)>>,
    pub info: Vec<Info>,
    pub current: ThreadId,
    live: usize,
    ticks: u32,
    pub io: Io,
}

/// Scheduling state of one thread, kept outside its record, which is taken while it runs.
#[derive(Default)]
pub struct Info {
    pub alive: bool,
    /// `LockSupport.park`'s permit.
    pub permit: bool,
    /// An interrupt not yet seen by a sleep or wait.
    pub interrupt: bool,
    pub blocked: Option<Wait>,
    /// Whether the last block ended by a wake rather than its deadline.
    pub woken: bool,
    /// Tells a blocked thread's timer from the timers of its earlier blocks; it keeps counting
    /// when the id is reused.
    generation: u64,
    /// The thread's `NativeThread` object.
    pub native: usize,
}

impl Sched {
    pub fn add(&mut self, id: ThreadId, native: usize) {
        if self.info.len() <= id {
            self.info.resize_with(id + 1, Info::default);
        }
        let generation = self.info[id].generation + 1;
        self.info[id] = Info { alive: true, native, generation, ..Info::default() };
        self.live += 1;
        self.queue.push_back(id);
    }

    /// Marks thread `id` ended and returns its `NativeThread` object.
    pub fn end(&mut self, id: ThreadId) -> usize {
        self.live -= 1;
        self.info[id].alive = false;
        self.info[id].native
    }

    /// Whether a thread besides the running one has not ended.
    pub fn others_alive(&self) -> bool {
        self.live > 1
    }

    /// Makes `id` runnable if it is blocked, and `true` if it was.
    pub fn wake(&mut self, id: ThreadId) -> bool {
        let info = &mut self.info[id];
        if info.blocked.take().is_none() {
            return false;
        }
        info.woken = true;
        self.queue.push_back(id);
        true
    }

    /// Wakes `id` only if it is blocked for `why`.
    pub fn wake_if(&mut self, id: ThreadId, why: Wait) -> bool {
        self.info[id].blocked == Some(why) && self.wake(id)
    }

    /// Makes runnable the threads whose deadline passed or whose blocking call finished.
    fn poll(&mut self) {
        let now = Instant::now();
        while let Some(&Reverse((deadline, id, generation))) = self.timers.peek() {
            if deadline > now {
                break;
            }
            self.timers.pop();
            let info = &mut self.info[id];
            if info.generation == generation && info.blocked.take().is_some() {
                info.woken = false;
                self.queue.push_back(id);
            }
        }
        for id in self.io.finished() {
            self.wake(id);
        }
    }

    /// The next thread to run, waiting for a timer or a blocking call if none is runnable;
    /// `None` when every thread waits without a deadline.
    pub fn next(&mut self) -> Option<ThreadId> {
        loop {
            self.poll();
            if let Some(id) = self.queue.pop_front() {
                return Some(id);
            }
            while let Some(&Reverse((_, id, generation))) = self.timers.peek() {
                let info = &self.info[id];
                if info.generation == generation && info.blocked.is_some() {
                    break;
                }
                self.timers.pop();
            }
            let timeout =
                self.timers.peek().map(|Reverse((d, ..))| d.saturating_duration_since(Instant::now()));
            match (timeout, self.io.busy()) {
                (None, false) => return None,
                (Some(t), false) => std::thread::sleep(t),
                (timeout, true) => {
                    if let Some(id) = self.io.wait(timeout) {
                        self.wake(id);
                    }
                }
            }
        }
    }
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

    /// Counts a call or loop back-edge, and lets other threads run every `TICKS` of them.
    #[inline]
    pub fn tick(&mut self) {
        let s = &mut self.threads.sched;
        s.ticks = s.ticks.wrapping_sub(1);
        if s.ticks > TICKS {
            s.ticks = TICKS;
            self.yield_now();
        }
    }
}
