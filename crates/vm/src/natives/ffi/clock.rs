//! The clocks: monotonic time from an arbitrary origin, and wall-clock time since the epoch.

use std::time::Duration;

/// Reads the monotonic clock, which never goes back; deadlines are points on it.
#[cfg(not(target_arch = "wasm32"))]
pub fn monotonic() -> Duration {
    read(libc::CLOCK_MONOTONIC)
}

/// Reads the wall clock: the time since 1970-01-01 UTC.
#[cfg(not(target_arch = "wasm32"))]
pub fn epoch() -> Duration {
    read(libc::CLOCK_REALTIME)
}

#[cfg(not(target_arch = "wasm32"))]
fn read(clock: libc::clockid_t) -> Duration {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid out-parameter and both clocks exist on every unix.
    unsafe { libc::clock_gettime(clock, &mut ts) };
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    /// The host's wall clock in milliseconds since the epoch, e.g. JavaScript's `Date.now()`.
    safe fn now_ms() -> f64;
}

/// Reads the monotonic clock: the host's wall clock, as the browser has no other.
#[cfg(target_arch = "wasm32")]
pub fn monotonic() -> Duration {
    epoch()
}

/// Reads the wall clock: the time since 1970-01-01 UTC.
#[cfg(target_arch = "wasm32")]
pub fn epoch() -> Duration {
    Duration::from_secs_f64(now_ms().max(0.0) / 1000.0)
}
