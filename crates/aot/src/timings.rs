//! The wall time of the phases of a build, which `--timings` prints.

use std::time::{Duration, Instant};

pub struct Timings {
    last: Instant,
    phases: Vec<(&'static str, Duration)>,
}

impl Timings {
    pub fn new() -> Self {
        Timings { last: Instant::now(), phases: Vec::new() }
    }

    /// Ends `phase`, which ran since the previous one ended.
    pub fn lap(&mut self, phase: &'static str) {
        let now = Instant::now();
        let spent = now.duration_since(self.last);
        self.last = now;
        self.add(phase, spent);
    }

    /// Adds `time` to `phase`, which may be a part of another.
    pub fn add(&mut self, phase: &'static str, time: Duration) {
        match self.phases.iter_mut().find(|p| p.0 == phase) {
            Some(p) => p.1 += time,
            None => self.phases.push((phase, time)),
        }
    }

    pub fn print(&self) {
        for (phase, time) in &self.phases {
            eprintln!("{phase:<24} {:>8.3} s", time.as_secs_f64());
        }
    }
}
