//! The running thread's `java.lang.Thread` and `NativeThread` state, read and written through
//! their Scala methods.

use crate::interp::Flow;
use crate::{Vm, heap};

const NATIVE_THREAD: &str = "scala.scalanative.runtime.NativeThread";
const STATE: &str = "L44scala.scalanative.runtime.NativeThread$State";

impl Vm {
    /// Sets the running thread's `NativeThread.state` to `NativeThread.State.<state>`, which
    /// `Thread.getState` reports, and returns the state before.
    pub fn set_thread_state(&mut self, state: &str) -> Flow {
        let module = self.world.top(&format!("{NATIVE_THREAD}$State${state}$"));
        let module = self.module(module)? as u64;
        self.put_thread_state(module)
    }

    /// Sets the running thread's `NativeThread.state` to the state object `state` and returns
    /// the state before.
    pub fn put_thread_state(&mut self, state: u64) -> Flow {
        let native: u64 = heap::read(self.thread + 8);
        if native == 0 || state == 0 {
            return Ok(0);
        }
        let owner = self.world.top(NATIVE_THREAD);
        let get = self.world.member(owner, &format!("D5state{STATE}EO"));
        let set = self.world.member(owner, &format!("D9state_$eq{STATE}uEO"));
        let before = self.call_global(get, &[native])?;
        self.call_global(set, &[native, state])?;
        Ok(before)
    }

    /// `Thread.currentThread().isInterrupted()`.
    pub fn is_interrupted(&mut self) -> Flow<bool> {
        let thread: u64 = heap::read(self.thread);
        if thread == 0 {
            return Ok(false);
        }
        let sig = nir::StrId(self.world.pool.strings.intern("D13isInterruptedzEO".into()));
        let f = self.virtual_method(thread as usize, sig)?;
        Ok(self.call(f, &[thread])? & 1 == 1)
    }

    /// `Thread.interrupted()`: whether the running thread was interrupted, clearing it.
    pub fn take_interrupt(&mut self) -> Flow<bool> {
        let thread = self.world.top("java.lang.Thread$");
        let module = self.module(thread)?;
        let interrupted = self.world.member(thread, "D11interruptedzEO");
        Ok(self.call_global(interrupted, &[module as u64])? & 1 == 1)
    }
}
