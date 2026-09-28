//! Where the instructions of a method with inlined callees came from, for stack traces.

use nir::{GlobalId, PosId};

/// Per instruction of an inlined body, the innermost callee it was copied from, if any.
pub struct Inlined {
    pub frames: Vec<InlineFrame>,
    /// Per NIR instruction, the index of its frame in `frames` plus one; 0 for the method's own.
    pub of: Vec<u32>,
}

/// A callee inlined at a call whose position is `call`, itself in frame `parent` (plus one;
/// 0 for the method's own code).
pub struct InlineFrame {
    pub method: GlobalId,
    pub call: PosId,
    pub parent: u32,
}

impl Inlined {
    /// A record for a body of `n` instructions of the method's own.
    pub fn own(n: usize) -> Self {
        Self { frames: Vec::new(), of: vec![0; n] }
    }

    /// The methods executing NIR instruction `inst` with the position each is at, innermost
    /// first and ending with `method` itself, when `inst` was inlined; `None` otherwise.
    pub fn chain(&self, method: GlobalId, inst: usize, pos: PosId) -> Option<Vec<(GlobalId, PosId)>> {
        let mut frame = *self.of.get(inst).filter(|&&f| f != 0)?;
        let mut out = Vec::new();
        let mut at = pos;
        while frame != 0 {
            let f = &self.frames[frame as usize - 1];
            out.push((f.method, at));
            at = f.call;
            frame = f.parent;
        }
        out.push((method, at));
        Some(out)
    }
}
