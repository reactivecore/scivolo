//! A count of the SIGINTs the process received and not yet taken (ADR 0013).

use std::sync::atomic::{AtomicU32, Ordering};

static PENDING: AtomicU32 = AtomicU32::new(0);

extern "C" fn on_sigint(_: libc::c_int) {
    PENDING.fetch_add(1, Ordering::Relaxed);
}

/// Counts SIGINTs instead of letting them end the process; blocking calls they interrupt fail
/// with `EINTR`. Calling it again changes nothing.
pub fn watch_sigint() -> Result<(), String> {
    // SAFETY: an all-zero `sigaction` is a valid value: no flags and an empty mask.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = on_sigint as extern "C" fn(libc::c_int) as libc::sighandler_t;
    action.sa_flags = 0;
    // SAFETY: `action` is initialized, its handler only touches an atomic, which is
    // async-signal-safe, and a null old action is allowed.
    if unsafe { libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut()) } != 0 {
        return Err(format!("sigaction: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

/// Whether a SIGINT arrived since the last call.
pub fn take_sigint() -> bool {
    PENDING.swap(0, Ordering::Relaxed) > 0
}
