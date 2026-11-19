//! The C functions the runtime calls, implemented in Rust for portable mode: memory and string
//! functions, atomics, the collector, clocks, `write` to stdout and stderr, `exit`, pthreads and
//! platform probes, as Linux answers them. Any other extern is refused as a missing native.

use super::clock;
use crate::natives::NativeFn;
use crate::{VmError, heap};
use std::time::Duration;

/// Bytes `scalanative_pthread_*_size` report: room for any platform's pthread object. The
/// pthread functions themselves do nothing, as one thread runs.
const PTHREAD_OBJECT_SIZE: u64 = 64;

/// The `sysconf` name of the online processor count; `sysconf` answers 1, the only processor.
const SC_NPROCESSORS_ONLN: u64 = 1;

/// Linux's `CLOCK_REALTIME`; every other clock id reads the monotonic clock.
const CLOCK_REALTIME: u64 = 0;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    if let Some(rest) = name.strip_prefix("scalanative_atomic_") {
        return super::atomics::extern_fn(rest);
    }
    let f: NativeFn = match name {
        "memcpy" | "memmove" => |_, a| {
            heap::copy(a[1] as usize, a[0] as usize, a[2] as usize);
            Ok(a[0])
        },
        "memset" => |_, a| {
            (0..a[2] as usize).for_each(|i| heap::write(a[0] as usize + i, a[1] as u8));
            Ok(a[0])
        },
        "memcmp" => |_, a| Ok(memcmp(a[0] as usize, a[1] as usize, a[2] as usize) as u32 as u64),
        "strlen" => |_, a| Ok(strlen(a[0] as usize) as u64),
        "memmem" => |_, a| Ok(memmem(a[0] as usize, a[1] as usize, a[2] as usize, a[3] as usize) as u64),
        "write" => |vm, a| match a[0] as i32 {
            fd @ (1 | 2) => {
                let bytes: Vec<u8> = (0..a[2] as usize).map(|i| heap::read(a[1] as usize + i)).collect();
                vm.record_output(fd, &bytes);
                Ok(a[2])
            }
            _ => Ok(-1i64 as u64),
        },
        "exit" => |_, a| Err(VmError::Exit(a[0] as i32).into()),
        "dlerror" | "scalanative_set_os_props" | "scalanative_stdin_fileno" => |_, _| Ok(0),
        "scalanative_stdout_fileno" => |_, _| Ok(1),
        "scalanative_stderr_fileno" => |_, _| Ok(2),
        "scalanative__sc_nprocessors_onln" => |_, _| Ok(SC_NPROCESSORS_ONLN),
        "sysconf" => |_, a| Ok(if a[0] as i32 as u64 == SC_NPROCESSORS_ONLN { 1 } else { -1i64 as u64 }),
        "scalanative_current_time_millis" => |_, _| Ok(clock::epoch().as_millis() as u64),
        "scalanative_nano_time" => |_, _| Ok(clock::monotonic().as_nanos() as u64),
        "scalanative_clock_realtime" => |_, _| Ok(CLOCK_REALTIME),
        "scalanative_clock_monotonic" => |_, _| Ok(1),
        "scalanative_clock_process_cputime_id" => |_, _| Ok(2),
        "scalanative_clock_thread_cputime_id" => |_, _| Ok(3),
        "clock_gettime" => |_, a| {
            let now = if a[0] as i32 as u64 == CLOCK_REALTIME { clock::epoch() } else { clock::monotonic() };
            write_timespec(a[1] as usize, now);
            Ok(0)
        },
        _ if name.starts_with("scalanative_pthread_") && name.ends_with("_size") => {
            |_, _| Ok(PTHREAD_OBJECT_SIZE)
        }
        _ if name.starts_with("pthread_") || name.starts_with("scalanative_pthread_") => |_, _| Ok(0),
        _ => return super::heap_externs::extern_fn(name),
    };
    Some(f)
}

/// Writes `t` as a C `struct timespec` of two 64-bit fields at `ts`.
fn write_timespec(ts: usize, t: Duration) {
    heap::write(ts, t.as_secs() as i64);
    heap::write(ts + 8, t.subsec_nanos() as i64);
}

fn strlen(s: usize) -> usize {
    (0..).find(|&i| heap::read::<u8>(s + i) == 0).expect("a terminated C string")
}

fn memcmp(a: usize, b: usize, len: usize) -> i32 {
    (0..len)
        .map(|i| heap::read::<u8>(a + i) as i32 - heap::read::<u8>(b + i) as i32)
        .find(|&d| d != 0)
        .unwrap_or(0)
}

fn memmem(hay: usize, hay_len: usize, needle: usize, needle_len: usize) -> usize {
    if needle_len > hay_len {
        return 0;
    }
    (0..=hay_len - needle_len).find(|&i| memcmp(hay + i, needle, needle_len) == 0).map_or(0, |i| hay + i)
}
