//! What the collector scans for green threads.

use crate::Vm;
use crate::heap::ThreadRoots;

/// Native stack below a suspended thread's recorded top that the switch itself may have used.
const SWITCH_MARGIN: usize = 4096;

impl Vm {
    /// Tells the collector about every frame stack, the native stacks of suspended threads and,
    /// while a thread runs, the scheduler's native stack.
    pub fn publish_roots(&mut self) {
        let t = &mut self.threads;
        t.roots.clear();
        t.roots.push(ThreadRoots { frames: self.frames.bounds(), low: 0, high: 0 });
        if let Some(r) = &t.running {
            t.roots.push(ThreadRoots { frames: r.frames, low: r.low, high: r.high });
        }
        for g in t.all.iter().flatten() {
            let (low, high) = g.co.bounds();
            let native = match g.sp {
                0 => (0, 0),
                sp => (sp.saturating_sub(SWITCH_MARGIN).max(low), high),
            };
            t.roots.push(ThreadRoots { frames: g.saved.frames.bounds(), low: native.0, high: native.1 });
        }
        self.heap.set_threads(&self.threads.roots);
    }
}
