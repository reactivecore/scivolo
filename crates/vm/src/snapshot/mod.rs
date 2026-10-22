//! What a heap snapshot of the trained region holds, and whether one may be taken; see ADR 0034.

mod walk;

use crate::Vm;

/// Modules, and classes of instances, whose state depends on the process environment.
const DENIED: [&str; 14] = [
    "java.lang.SystemProperties$",
    "java.lang.EnvVars$",
    "java.lang.Runtime$",
    "java.lang.ProcessBuilder",
    "java.util.Locale$",
    "java.util.TimeZone$",
    "java.util.Random",
    "java.util.concurrent.ThreadLocalRandom$",
    "java.util.concurrent.ForkJoinPool$",
    "java.net.InetAddress$",
    "java.io.File$",
    "scala.util.Random$",
    "scala.scalanative.runtime.StackTrace$",
    "scala.scalanative.runtime.ThreadStackSize$",
];

/// A word the restoring process rewrites to the address of what it names.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// The C trampoline of the function, top-level name and member signature.
    Trampoline(String, String),
    /// The VM's reference to the function.
    Function(String, String),
    /// A C symbol.
    Symbol(String),
}

/// The trained region: its offsets that hold a `Target`, and how many objects it holds.
#[derive(Debug, Default)]
pub struct Walk {
    pub bytes: usize,
    pub objects: usize,
    pub targets: Vec<(usize, Target)>,
}

impl Vm {
    /// The walk of the region the runtime's initialization filled, or why it cannot be a
    /// snapshot.
    pub fn snapshot_walk(&self) -> Result<Walk, String> {
        let trained = self.heap.trained().ok_or("no initialization ran in the region")?;
        let pool = &self.world.pool;
        if let Some(m) = self.modules.keys().map(|&g| pool.top_name(g)).find(|m| DENIED.contains(m)) {
            return Err(format!("module {m} depends on the environment"));
        }
        if !self.threads.initializing.is_empty() || !self.threads.monitors.is_empty() {
            return Err("a module initializer or monitor is still held".into());
        }
        if !self.funcs.consts.is_empty() {
            return Err("constant data is keyed by this process's definitions".into());
        }
        let props = self.linktime.props().map(|v| v as usize).filter(|v| trained.range.contains(v));
        let roots: Vec<usize> = (self.modules.values().chain(self.strings.values()).copied())
            .chain(self.classes.all.iter().map(|c| c.rtti))
            .chain(self.funcs.globals.values().copied())
            .chain(props)
            .chain([self.thread])
            .collect();
        walk::walk(self, trained, &roots)
    }
}
