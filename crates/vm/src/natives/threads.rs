//! Java threads on the VM's green threads: thread creation, parking, sleeping, interrupts,
//! monitors and `wait`/`notify` replace their pthread-based implementations.

use super::NativeFn;
use crate::interp::Flow;
use crate::threads::Wait;
use crate::{Vm, heap};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    match name {
        "scalanative_GC_pthread_create" => Some(pthread_create),
        _ => None,
    }
}

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        ("java.lang.impl.PosixThread", "D4parkjzuEO") => park,
        ("java.lang.impl.PosixThread", "D6unparkuEO") => |vm, args| {
            unpark(vm, args[0] as usize, false);
            Ok(0)
        },
        ("java.lang.impl.PosixThread", "D5sleepjuEO") => sleep,
        ("java.lang.impl.PosixThread", "D10sleepNanosiuEO") => |vm, args| {
            let deadline = Instant::now() + Duration::from_nanos(args[1] as i32 as u64);
            let before = vm.set_thread_state("ParkedWaitingTimed")?;
            vm.block(Wait::Sleep, Some(deadline));
            vm.put_thread_state(before)
        },
        ("java.lang.impl.PosixThread", "D9interruptuEO") => |vm, args| {
            unpark(vm, args[0] as usize, true);
            Ok(0)
        },
        ("java.lang.impl.PosixThread", "D11setPriorityiuEO") => |_, _| Ok(0),
        ("java.lang.impl.PosixThread", "D11yieldThreaduEo")
        | ("java.lang.impl.PosixThread$", "D11yieldThreaduEO")
        | ("scala.scalanative.runtime.NativeThread", "D10onSpinWaituEo")
        | ("scala.scalanative.runtime.NativeThread$", "D10onSpinWaituEO") => |vm, _| {
            vm.yield_now();
            Ok(0)
        },
        ("scala.scalanative.runtime.package$", "D12enterMonitorL16java.lang.ObjectuEO") => {
            |vm, args| vm.monitor_enter(args[1] as usize).map(|()| 0)
        }
        ("scala.scalanative.runtime.package$", "D11exitMonitorL16java.lang.ObjectuEO") => {
            |vm, args| vm.monitor_exit(args[1] as usize).map(|()| 0)
        }
        ("scala.scalanative.runtime.NativeThread", "D9holdsLockL16java.lang.ObjectzEo") => {
            |vm, args| Ok(vm.holds_monitor(args[0] as usize) as u64)
        }
        ("scala.scalanative.runtime.NativeThread$", "D9holdsLockL16java.lang.ObjectzEO") => {
            |vm, args| Ok(vm.holds_monitor(args[1] as usize) as u64)
        }
        ("java.lang.Object", "D4waituEO") => |vm, args| vm.monitor_wait(args[0] as usize, None).map(|()| 0),
        ("java.lang.Object", "D4waitjuEO") => |vm, args| wait(vm, args[0], args[1] as i64, 0),
        ("java.lang.Object", "D4waitjiuEO") => |vm, args| wait(vm, args[0], args[1] as i64, args[2] as i32),
        ("java.lang.Object", "D6notifyuEO") => {
            |vm, args| vm.monitor_notify(args[0] as usize, false).map(|()| 0)
        }
        ("java.lang.Object", "D9notifyAlluEO") => {
            |vm, args| vm.monitor_notify(args[0] as usize, true).map(|()| 0)
        }
        ("scala.scalanative.meta.LinktimeInfo$", "D24isContinuationsSupportedzEO") => |_, _| Ok(0),
        _ => return None,
    };
    Some(f)
}

/// `scalanative_GC_pthread_create(thread, attr, routine, arg)`: a green thread that runs
/// `routine(arg)`; `arg` is the new thread's `NativeThread` object.
fn pthread_create(vm: &mut Vm, args: &[u64]) -> Flow {
    let (routine, arg) = (args[2], args[3]);
    vm.start_thread(arg as usize, move |vm| vm.call_value(routine, &[arg]).map(|_| ()))?;
    heap::write(args[0] as usize, arg);
    Ok(0)
}

/// `PosixThread.park(time, isAbsolute)`: `time` is a deadline in epoch milliseconds if
/// absolute, else nanoseconds; 0 and relative means no deadline.
fn park(vm: &mut Vm, args: &[u64]) -> Flow {
    let (time, absolute) = (args[1] as i64, args[2] & 1 == 1);
    let id = vm.threads.sched.current;
    if std::mem::take(&mut vm.threads.sched.info[id].permit) || time < 0 || (absolute && time == 0) {
        return Ok(0);
    }
    if vm.is_interrupted()? {
        return Ok(0);
    }
    let deadline = match (time, absolute) {
        (0, false) => None,
        (nanos, false) => Some(Instant::now() + Duration::from_nanos(nanos as u64)),
        (millis, true) => {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
            Some(Instant::now() + Duration::from_millis((millis - now).max(0) as u64))
        }
    };
    let before =
        vm.set_thread_state(if deadline.is_some() { "ParkedWaitingTimed" } else { "ParkedWaiting" })?;
    if !std::mem::take(&mut vm.threads.sched.info[id].permit) {
        vm.block(Wait::Park, deadline);
    }
    vm.threads.sched.info[id].permit = false;
    vm.put_thread_state(before)
}

/// Gives `native`'s thread a park permit and wakes it from parking or, for an interrupt, from
/// sleeping or waiting.
fn unpark(vm: &mut Vm, native: usize, interrupt: bool) {
    let Some(&id) = vm.threads.by_native.get(&native) else { return };
    let info = &mut vm.threads.sched.info[id];
    info.permit = true;
    info.interrupt |= interrupt;
    let wakes = match info.blocked {
        Some(Wait::Park) => true,
        Some(Wait::Sleep | Wait::Notify) => interrupt,
        _ => false,
    };
    if wakes {
        vm.threads.sched.wake(id);
    }
}

/// `PosixThread.sleep(millis)`, ended early by an interrupt.
fn sleep(vm: &mut Vm, args: &[u64]) -> Flow {
    let id = vm.threads.sched.current;
    vm.threads.sched.info[id].interrupt = false;
    if vm.take_interrupt()? {
        return vm.throw_new("java.lang.InterruptedException");
    }
    let deadline = Instant::now() + Duration::from_millis(args[1] as i64 as u64);
    let before = vm.set_thread_state("ParkedWaitingTimed")?;
    let mut interrupted = false;
    while !interrupted && Instant::now() < deadline {
        interrupted = std::mem::take(&mut vm.threads.sched.info[id].interrupt);
        if !interrupted {
            vm.block(Wait::Sleep, Some(deadline));
        }
    }
    vm.put_thread_state(before)?;
    if interrupted && vm.take_interrupt()? {
        return vm.throw_new("java.lang.InterruptedException");
    }
    Ok(0)
}

/// `obj.wait(millis, nanos)`; no timeout if both are 0.
fn wait(vm: &mut Vm, obj: u64, millis: i64, nanos: i32) -> Flow {
    if millis < 0 || !(0..=999_999).contains(&nanos) {
        return vm.throw_new("java.lang.IllegalArgumentException");
    }
    let timeout = Duration::from_millis(millis as u64) + Duration::from_nanos(nanos as u64);
    let deadline = (!timeout.is_zero()).then(|| Instant::now() + timeout);
    vm.monitor_wait(obj as usize, deadline).map(|()| 0)
}
