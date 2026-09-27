//! Stackful coroutines, one per green thread. Every switch passes the VM's address along, so the
//! compiler treats the VM as changed by the other side.

use crate::Vm;
use crate::interp::Flow;
use corosensei::stack::{DefaultStack, Stack};
use corosensei::{CoroutineResult, Yielder};

type Inner = corosensei::Coroutine<usize, usize, Flow<()>, DefaultStack>;

pub struct Coroutine {
    inner: Inner,
    low: usize,
    high: usize,
}

impl Coroutine {
    /// A coroutine on `size` bytes of stack that runs `body(vm, yielder)` when first resumed;
    /// `yielder` is what `suspend` takes.
    pub fn new(size: usize, body: impl FnOnce(&mut Vm, usize) -> Flow<()> + 'static) -> Result<Self, String> {
        let stack = DefaultStack::new(size).map_err(|e| format!("cannot allocate a thread stack: {e}"))?;
        let (low, high) = (stack.limit().get(), stack.base().get());
        let inner = Inner::with_stack(stack, move |yielder: &Yielder<usize, usize>, vm: usize| {
            // SAFETY: `resume` passes the VM that resumes the coroutine; it outlives the coroutine,
            // and the resuming code does not use it until the coroutine suspends or returns, the
            // same hand-over C callbacks use (ADR 0002).
            let vm = unsafe { &mut *(vm as *mut Vm) };
            body(vm, yielder as *const Yielder<usize, usize> as usize)
        });
        Ok(Self { inner, low, high })
    }

    /// The lowest and highest address of the coroutine's stack.
    pub fn bounds(&self) -> (usize, usize) {
        (self.low, self.high)
    }

    /// Runs the coroutine with `vm` until it suspends (`None`) or returns its result.
    pub fn resume(&mut self, vm: *mut Vm) -> Option<Flow<()>> {
        match self.inner.resume(vm as usize) {
            CoroutineResult::Yield(_) => None,
            CoroutineResult::Return(result) => Some(result),
        }
    }
}

/// Suspends the running coroutine whose body received `yielder` until it is resumed, handing
/// `vm` to the resumer.
pub fn suspend(yielder: usize, vm: *mut Vm) {
    // SAFETY: `yielder` is the address of the `Yielder` the running coroutine's body received,
    // which lives on that coroutine's stack while the body runs.
    let yielder = unsafe { &*(yielder as *const Yielder<usize, usize>) };
    std::hint::black_box(yielder.suspend(vm as usize));
}

/// An address near the top of the current native stack.
#[inline(never)]
pub fn approximate_sp() -> usize {
    let marker = 0u8;
    std::hint::black_box(&raw const marker as usize)
}
