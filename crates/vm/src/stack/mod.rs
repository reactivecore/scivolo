//! Stacks: the bounds of the OS thread's native stack, the interpreter's frame stacks, and the
//! coroutines green threads run on.

mod coroutine;
mod frames;

pub use coroutine::{Coroutine, approximate_sp, suspend};
pub use frames::Frames;

/// The lowest and highest address of the current thread's stack, if the platform tells.
#[cfg(target_os = "macos")]
pub fn bounds() -> Option<(usize, usize)> {
    // SAFETY: both functions only read the calling thread's own attributes.
    let (high, size) = unsafe {
        let me = libc::pthread_self();
        (libc::pthread_get_stackaddr_np(me) as usize, libc::pthread_get_stacksize_np(me))
    };
    Some((high.checked_sub(size)?, high))
}

/// The lowest and highest address of the current thread's stack, if the platform tells.
#[cfg(target_os = "linux")]
pub fn bounds() -> Option<(usize, usize)> {
    let mut attr = std::mem::MaybeUninit::<libc::pthread_attr_t>::uninit();
    let (mut addr, mut size) = (std::ptr::null_mut(), 0);
    // SAFETY: `attr` is initialized by `pthread_getattr_np` before it is read and destroyed
    // after; `addr` and `size` are valid out-parameters.
    let ok = unsafe {
        libc::pthread_getattr_np(libc::pthread_self(), attr.as_mut_ptr()) == 0 && {
            let got = libc::pthread_attr_getstack(attr.as_ptr(), &mut addr, &mut size) == 0;
            libc::pthread_attr_destroy(attr.as_mut_ptr());
            got
        }
    };
    ok.then_some((addr as usize, addr as usize + size))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn bounds() -> Option<(usize, usize)> {
    None
}
