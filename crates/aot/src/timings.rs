//! The wall time of the phases of a build and of parts of them, which `--timings` prints.

use std::time::{Duration, Instant};

pub struct Timings {
    last: Instant,
    phases: Vec<(&'static str, Duration)>,
    /// Parts of phases: phase, part, time.
    parts: Vec<(&'static str, &'static str, Duration)>,
}

impl Timings {
    pub fn new() -> Self {
        Timings { last: Instant::now(), phases: Vec::new(), parts: Vec::new() }
    }

    /// Ends `phase`, which ran since the previous one ended.
    pub fn lap(&mut self, phase: &'static str) {
        let now = Instant::now();
        self.phases.push((phase, now.duration_since(self.last)));
        self.last = now;
    }

    /// Adds `time` to `part` of `phase`.
    pub fn part(&mut self, phase: &'static str, part: &'static str, time: Duration) {
        match self.parts.iter_mut().find(|p| p.0 == phase && p.1 == part) {
            Some(p) => p.2 += time,
            None => self.parts.push((phase, part, time)),
        }
    }

    pub fn print(&self) {
        for (phase, time) in &self.phases {
            eprintln!("{phase:<24} {:>8.3} s", time.as_secs_f64());
            for (_, part, time) in self.parts.iter().filter(|p| p.0 == *phase) {
                eprintln!("  of which {part:<14} {:>8.3} s", time.as_secs_f64());
            }
        }
    }
}
