//! The bounds of the current thread's native stack, and the interpreter's frame stack.

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

/// Address space reserved for frames; pages are committed as deep calls touch them.
const FRAMES: usize = 1 << 30;
const ALIGN: usize = 16;

/// The end of the frame stack only `StackOverflowError` construction may use.
const RESERVE: usize = 16 << 20;

/// A frame's header: the header of the enclosing frame, the frame's method and the index of the
/// instruction it executes.
const HEADER: usize = 24;

/// The slots, `stackalloc` memory and escaping variables of every active interpreted call, in
/// one region: `[0]` is its low end, `[1]` the top, which the collector scans up to.
pub struct Frames {
    bounds: Box<[usize; 2]>,
    end: usize,
    reserve_open: bool,
    innermost: usize,
}

/// A state of the frame stack to return to.
#[derive(Clone, Copy)]
pub struct Mark {
    top: usize,
    innermost: usize,
}

impl Frames {
    pub fn new() -> Result<Self, String> {
        // SAFETY: an anonymous private mapping touches no existing memory; it is never unmapped,
        // so slices into it stay valid for the process's lifetime.
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                FRAMES,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err("cannot reserve the frame stack".into());
        }
        let low = base as usize;
        Ok(Self { bounds: Box::new([low, low]), end: low + FRAMES, reserve_open: false, innermost: 0 })
    }

    /// Lets pushes use the reserve, or stops them.
    pub fn open_reserve(&mut self, open: bool) {
        self.reserve_open = open;
    }

    /// The bounds the collector reads, `[low, top]`.
    pub fn bounds(&self) -> *const usize {
        self.bounds.as_ptr()
    }

    pub fn mark(&self) -> Mark {
        Mark { top: self.bounds[1], innermost: self.innermost }
    }

    /// Frees everything pushed since `mark`, and returns to its innermost frame.
    pub fn reset(&mut self, mark: Mark) {
        self.bounds[1] = mark.top;
        self.innermost = mark.innermost;
    }

    /// Pushes a frame of method `method`: a header linking to the enclosing frame, then `len`
    /// zeroed slots, whose address it returns.
    pub fn push_frame(&mut self, method: u64, len: usize) -> Option<usize> {
        let at = self.push(HEADER + 8 * len)?;
        crate::heap::write(at, self.innermost);
        crate::heap::write(at + 8, method);
        self.innermost = at;
        Some(at + HEADER)
    }

    /// The header of the innermost frame, 0 when there is none.
    pub fn innermost(&self) -> usize {
        self.innermost
    }

    /// The header of the frame enclosing the one at `header`, 0 when there is none.
    pub fn caller(header: usize) -> usize {
        crate::heap::read(header)
    }

    /// The method of the frame at `header`.
    pub fn method(header: usize) -> u64 {
        crate::heap::read(header + 8)
    }

    /// Records that the innermost frame executes instruction `pc`.
    #[inline]
    pub fn set_pc(&mut self, pc: usize) {
        debug_assert!(self.innermost != 0, "no frame to record a pc in");
        crate::heap::write(self.innermost + 16, pc as u64);
    }

    /// The instruction the frame at `header` executes.
    pub fn pc(header: usize) -> usize {
        crate::heap::read::<u64>(header + 16) as usize
    }

    /// `bytes` zeroed bytes aligned to 16 on top, or `None` when the region is full.
    pub fn push(&mut self, bytes: usize) -> Option<usize> {
        let at = self.bounds[1].next_multiple_of(ALIGN);
        let limit = if self.reserve_open { self.end } else { self.end - RESERVE };
        let top = at.checked_add(bytes).filter(|&t| t <= limit)?;
        // SAFETY: `at..top` lies inside the mapping and above every live frame.
        unsafe { std::ptr::write_bytes(at as *mut u8, 0, bytes) };
        self.bounds[1] = top;
        Some(at)
    }

    /// The `len` slots pushed at `at`.
    pub fn slots<'a>(&self, at: usize, len: usize) -> &'a mut [u64] {
        debug_assert!(at >= self.bounds[0] && at + 8 * len <= self.bounds[1]);
        // SAFETY: `at` came from `push` of at least `8 * len` bytes; the region is never unmapped
        // and later pushes lie above it, so the slice aliases nothing until its frame is
        // reset, after which its call no longer uses it.
        unsafe { std::slice::from_raw_parts_mut(at as *mut u64, len) }
    }
}
