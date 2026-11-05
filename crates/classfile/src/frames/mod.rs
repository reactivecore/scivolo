//! Stack map frames by a dataflow over a method's code (ASM's `COMPUTE_FRAMES`).

mod encode;
mod execute;
mod lattice;

pub use lattice::Lub;

use crate::{Code, Pool};
use execute::{execute, length, target};
use lattice::{Frame, VType};
use std::collections::BTreeSet;

/// A method's class, internal name, and its static flag, name and descriptor.
pub struct Method<'a> {
    pub class: &'a str,
    pub is_static: bool,
    pub name: &'a str,
    pub descriptor: &'a str,
}

/// The body of the `StackMapTable` of `code`, none if it needs no frames. Unreachable code becomes
/// `nop`s ending in `athrow`.
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
    let (mut starts, mut targets, mut at) = (BTreeSet::from([0]), BTreeSet::new(), 0);
    while at < bytes.len() {
        let next = at + length(bytes, at);
        if let Some(t) = target(bytes, at) {
            targets.insert(t);
            starts.insert(t);
        }
        if target(bytes, at).is_some() || matches!(bytes[at], 0xac..=0xb1 | 0xbf) {
            starts.insert(next);
        }
        at = next;
    }
    let starts: Vec<usize> = starts.into_iter().filter(|&s| s < bytes.len()).collect();
    let initial = Frame::initial(m.class, m.is_static, m.name, m.descriptor, code.max_locals.into());
    let mut ins: Vec<Option<Frame>> = vec![None; starts.len()];
    ins[0] = Some(initial.clone());
    let mut work = vec![0];
    while let Some(b) = work.pop() {
        let mut f = ins[b].clone().expect("a frame at a block on the work list");
        let end = starts.get(b + 1).copied().unwrap_or(bytes.len());
        let (mut at, mut last) = (starts[b], starts[b]);
        while at < end {
            last = at;
            execute(&mut f, bytes, at, pool, m.class)?;
            at += length(bytes, at);
        }
        let falls = !matches!(bytes[last], 0xa7 | 0xac..=0xb1 | 0xbf) && end < bytes.len();
        for s in target(bytes, last).into_iter().chain(falls.then_some(end)) {
            let i = starts.binary_search(&s).map_err(|_| "a jump into an instruction")?;
            match &mut ins[i] {
                Some(into) => {
                    if into.merge(&f, lub)? {
                        work.push(i)
                    }
                }
                none => {
                    *none = Some(f.clone());
                    work.push(i)
                }
            }
        }
    }
    let mut frames = Vec::new();
    for (i, &start) in starts.iter().enumerate() {
        match ins[i].take() {
            Some(f) if targets.contains(&start) => frames.push((start, f)),
            Some(_) => {}
            None => {
                let end = starts.get(i + 1).copied().unwrap_or(code.bytes.len());
                code.bytes[start..end].fill(0x00);
                code.bytes[end - 1] = 0xbf;
                code.max_stack = code.max_stack.max(1);
                let throwable = VType::Object("java/lang/Throwable".into());
                frames.push((start, Frame { locals: Vec::new(), stack: vec![throwable] }));
            }
        }
    }
    Ok((!frames.is_empty()).then(|| encode::stack_map_table(&frames, &initial, pool)))
}
