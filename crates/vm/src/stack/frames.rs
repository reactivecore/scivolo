//! The interpreter's frame stack: slots, `stackalloc` memory and escaping variables of every
//! active call of one thread.

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
    size: usize,
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
    /// A frame stack of `size` bytes of address space, more than `RESERVE`; pages are committed
    /// as deep calls touch them.
    pub fn new(size: usize) -> Result<Self, String> {
        assert!(size > 2 * RESERVE, "frame stack of {size} bytes");
        // SAFETY: an anonymous private mapping touches no existing memory; it is unmapped only
        // when the `Frames` is dropped, after its thread's calls have ended.
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err("cannot reserve a frame stack".into());
        }
        let low = base as usize;
        let bounds = Box::new([low, low]);
        Ok(Self { bounds, size, end: low + size, reserve_open: false, innermost: 0 })
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
    /// slots, whose address it returns. The slots keep what earlier frames left there.
    #[inline]
    pub fn push_frame(&mut self, method: u64, len: usize) -> Option<usize> {
        let at = self.bounds[1].next_multiple_of(ALIGN);
        let limit = if self.reserve_open { self.end } else { self.end - RESERVE };
        let top = at.checked_add(HEADER + 8 * len).filter(|&t| t <= limit)?;
        // SAFETY: `at..at + HEADER` lies inside the mapping, above every live frame, and is
        // 16-byte aligned.
        unsafe { (at as *mut [u64; 3]).write([self.innermost as u64, method, 0]) };
        self.bounds[1] = top;
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
        // SAFETY: `innermost` is the header `push_frame` wrote, whose third word is the pc.
        unsafe { ((self.innermost + 16) as *mut u64).write(pc as u64) };
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
        // SAFETY: `at` came from `push` of at least `8 * len` bytes; the region stays mapped while
        // the calls of its thread run and later pushes lie above it, so the slice aliases nothing
        // until its frame is reset, after which its call no longer uses it.
        unsafe { std::slice::from_raw_parts_mut(at as *mut u64, len) }
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        // SAFETY: the mapping came from `mmap` in `new` with this size; the thread's calls have
        // ended, so nothing refers into it.
        unsafe { libc::munmap(self.bounds[0] as *mut libc::c_void, self.size) };
    }
}
