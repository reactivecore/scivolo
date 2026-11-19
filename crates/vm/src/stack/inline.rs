//! Green threads in portable mode: the only thread, the main one, runs to its end on the
//! native stack of the code that resumes it.

use crate::Vm;
use crate::interp::Flow;

type Body = Box<dyn FnOnce(&mut Vm, usize) -> Flow<()>>;

pub struct Coroutine {
    body: Option<Body>,
}

impl Coroutine {
    /// A coroutine that runs `body(vm, 0)` when resumed; `0` means it cannot suspend.
    pub fn new(
        _size: usize,
        body: impl FnOnce(&mut Vm, usize) -> Flow<()> + 'static,
    ) -> Result<Self, String> {
        Ok(Self { body: Some(Box::new(body)) })
    }

    /// Returns empty bounds: the body runs on the resumer's stack.
    pub fn bounds(&self) -> (usize, usize) {
        (0, 0)
    }

    /// Runs the body with `vm` to its end and returns its result.
    pub fn resume(&mut self, vm: *mut Vm) -> Option<Flow<()>> {
        let body = self.body.take().expect("a portable coroutine runs once");
        // SAFETY: `vm` is the VM that resumes the coroutine; it outlives the call, and the
        // resuming code does not use it until the body returns (ADR 0002's hand-over).
        Some(body(unsafe { &mut *vm }, 0))
    }
}

/// Never called: a body that received yielder `0` does not suspend.
pub fn suspend(_yielder: usize, _vm: *mut Vm) {
    unreachable!("portable mode suspends no thread");
}
