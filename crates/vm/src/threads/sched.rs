//! The scheduler: a run queue of green threads, their timers and waking.

use super::ThreadId;
use super::io::Io;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::time::{Duration, Instant};

/// What the scheduler does next.
pub enum Next {
    Run(ThreadId),
    /// A wait ended at its slice with no thread runnable.
    Waited,
    /// Every thread waits without a deadline.
    Deadlock,
}

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
    pub queue: VecDeque<ThreadId>,
    pub timers: BinaryHeap<Reverse<(Instant, ThreadId, u64)>>,
    pub info: Vec<Info>,
    pub current: ThreadId,
    live: usize,
    /// Calls and loop back-edges left until the next preemption check.
    pub ticks: u32,
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
    pub generation: u64,
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
    pub fn poll(&mut self) {
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

    /// The next thread to run, waiting for a timer or a blocking call if none is runnable; a
    /// wait longer than `slice` ends early with `Next::Waited`.
    pub fn next(&mut self, slice: Option<Duration>) -> Next {
        loop {
            self.poll();
            if let Some(id) = self.queue.pop_front() {
                return Next::Run(id);
            }
            while let Some(&Reverse((_, id, generation))) = self.timers.peek() {
                let info = &self.info[id];
                if info.generation == generation && info.blocked.is_some() {
                    break;
                }
                self.timers.pop();
            }
            let timer =
                self.timers.peek().map(|Reverse((d, ..))| d.saturating_duration_since(Instant::now()));
            let busy = self.io.busy();
            if timer.is_none() && !busy {
                return Next::Deadlock;
            }
            let timeout = match slice {
                Some(s) => Some(timer.map_or(s, |t| t.min(s))),
                None => timer,
            };
            if !busy {
                std::thread::sleep(timeout.unwrap_or_default());
            } else if let Some(id) = self.io.wait(timeout) {
                self.wake(id);
            }
            if slice.is_some() {
                return Next::Waited;
            }
        }
    }
}
