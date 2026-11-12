//! Stack map frames by a dataflow over a method's code (ASM's `COMPUTE_FRAMES`).

mod encode;
mod execute;
mod flow;
mod lattice;

pub use lattice::Lub;

use crate::{Code, Pool};
use execute::execute;
pub use flow::length;
use flow::{live_ranges, targets};
use lattice::{Frame, VType};
use std::collections::BTreeSet;

/// A method's class, internal name, and its static flag, name and descriptor.
pub struct Method<'a> {
    pub class: &'a str,
    pub is_static: bool,
    pub name: &'a str,
    pub descriptor: &'a str,
}

/// The frames entering the basic blocks starting at `starts`, and the blocks still to run.
struct Flow<'s> {
    starts: &'s [usize],
    ins: Vec<Option<Frame>>,
    work: Vec<usize>,
}

impl Flow<'_> {
    /// Merges `f` into the frame entering the block at offset `at`.
    fn reach(&mut self, at: usize, f: &Frame, lub: &mut Lub) -> Result<(), String> {
        let i = self.starts.binary_search(&at).map_err(|_| "a jump into an instruction")?;
        match &mut self.ins[i] {
            Some(into) => {
                if into.merge(f, lub)? {
                    self.work.push(i)
                }
            }
            none => {
                *none = Some(f.clone());
                self.work.push(i)
            }
        }
        Ok(())
    }
}

/// The body of the `StackMapTable` of `code`, none if it needs no frames; sets its exception
/// table. Unreachable code becomes `nop`s ending in `athrow`, outside the protected ranges.
pub fn stack_map(
    code: &mut Code,
    pool: &mut Pool,
    m: &Method,
    lub: &mut Lub,
) -> Result<Option<Vec<u8>>, String> {
    let bytes = &code.bytes;
    if bytes.is_empty() {
        return Err("no code".into());
    }
    let handlers = code.handler_offsets().ok_or("a protected range with a label never placed")?;
    let mut starts = BTreeSet::from([0]);
    let mut jumped: BTreeSet<usize> = handlers.iter().map(|h| h[2]).collect();
    starts.extend(&jumped);
    starts.extend(handlers.iter().flat_map(|h| [h[0], h[1]]));
    let mut at = 0;
    while at < bytes.len() {
        let to = targets(bytes, at);
        jumped.extend(&to);
        starts.extend(to);
        at += length(bytes, at);
    }
    let labels = starts.clone();
    let (mut at, mut in_block) = (0, true);
    while at < bytes.len() {
        in_block |= labels.contains(&at);
        let next = at + length(bytes, at);
        let ends = matches!(bytes[at], 0xa7 | 0xaa | 0xab | 0xac..=0xb1 | 0xbf | 0xc8);
        if in_block && (ends || !targets(bytes, at).is_empty()) {
            starts.insert(next);
        }
        in_block &= !ends;
        at = next;
    }
    let starts: Vec<usize> = starts.into_iter().filter(|&s| s < bytes.len()).collect();
    let catches: Vec<VType> = handlers
        .iter()
        .map(|h| {
            VType::Object(if h[3] == 0 { "java/lang/Throwable".into() } else { pool.class_name(h[3] as u16) })
        })
        .collect();
    let initial = Frame::initial(m.class, m.is_static, m.name, m.descriptor, code.max_locals.into());
    let mut flow = Flow { starts: &starts, ins: vec![None; starts.len()], work: vec![0] };
    flow.ins[0] = Some(initial.clone());
    while let Some(b) = flow.work.pop() {
        let mut f = flow.ins[b].clone().expect("a frame at a block on the work list");
        let end = starts.get(b + 1).copied().unwrap_or(bytes.len());
        let (mut at, mut last) = (starts[b], starts[b]);
        while at < end {
            last = at;
            for (h, catch) in handlers.iter().zip(&catches).filter(|(h, _)| (h[0]..h[1]).contains(&at)) {
                let thrown = Frame { locals: f.locals.clone(), stack: vec![catch.clone()] };
                flow.reach(h[2], &thrown, lub)?;
            }
            execute(&mut f, bytes, at, pool, m.class)?;
            at += length(bytes, at);
        }
        let falls = !matches!(bytes[last], 0xa7 | 0xaa..=0xb1 | 0xbf | 0xc8) && end < bytes.len();
        for s in targets(bytes, last).into_iter().chain(falls.then_some(end)) {
            flow.reach(s, &f, lub)?;
        }
    }
    let (mut frames, mut dead) = (Vec::new(), Vec::new());
    for (i, &start) in starts.iter().enumerate() {
        match flow.ins[i].take() {
            Some(f) if jumped.contains(&start) => frames.push((start, f)),
            Some(_) => {}
            None => {
                let end = starts.get(i + 1).copied().unwrap_or(code.bytes.len());
                code.bytes[start..end].fill(0x00);
                code.bytes[end - 1] = 0xbf;
                code.max_stack = code.max_stack.max(1);
                dead.push((start, end));
                let throwable = VType::Object("java/lang/Throwable".into());
                frames.push((start, Frame { locals: Vec::new(), stack: vec![throwable] }));
            }
        }
    }
    code.exceptions = live_ranges(&handlers, &dead);
    Ok((!frames.is_empty()).then(|| encode::stack_map_table(&frames, &initial, pool)))
}
