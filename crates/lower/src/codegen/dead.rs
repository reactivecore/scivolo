//! Blocks the entry does not reach, removed (NIR's `ControlFlow.removeDeadBlocks`).

use super::buf::Buf;
use nir::{Inst, Local, Next};
use rustc_hash::{FxHashMap, FxHashSet};

/// The blocks of `buf` reachable from its first, in their order.
pub fn remove_dead_blocks(buf: Buf) -> Buf {
    let starts: FxHashMap<Local, usize> = buf
        .insts
        .iter()
        .enumerate()
        .filter_map(|(i, inst)| match inst {
            Inst::Label(id, _) => Some((*id, i)),
            _ => None,
        })
        .collect();
    let end = |start: usize| {
        let cf = buf.insts[start + 1..].iter().position(|i| !matches!(i, Inst::Label(..) | Inst::Let { .. }));
        start + 1 + cf.expect("a block ends in control flow")
    };
    let Some(Inst::Label(entry, _)) = buf.insts.first() else { unreachable!("an entry label") };
    let mut visited = FxHashSet::default();
    let mut todo = vec![*entry];
    while let Some(block) = todo.pop() {
        if !visited.insert(block) {
            continue;
        }
        let start = starts[&block];
        for inst in &buf.insts[start + 1..=end(start)] {
            match inst {
                Inst::Let { unwind, .. } => successor(unwind, &mut todo),
                Inst::Jump(n) | Inst::Throw(_, n) | Inst::Unreachable(n) => successor(n, &mut todo),
                Inst::If(_, a, b) | Inst::LinktimeIf(_, a, b) => {
                    successor(a, &mut todo);
                    successor(b, &mut todo);
                }
                Inst::Switch(_, default, cases) => {
                    successor(default, &mut todo);
                    cases.iter().for_each(|c| successor(c, &mut todo));
                }
                Inst::Ret(_) | Inst::Label(..) => {}
            }
        }
    }
    let mut out = Buf::default();
    let mut kept: Vec<usize> = starts.iter().filter(|(b, _)| visited.contains(*b)).map(|(_, &s)| s).collect();
    kept.sort_unstable();
    for start in kept {
        for i in start..=end(start) {
            out.insts.push(buf.insts[i].clone());
            out.positions.push(buf.positions[i]);
        }
    }
    out
}

/// The block `next` leads to, if any.
fn successor(next: &Next, todo: &mut Vec<Local>) {
    match next {
        Next::None => {}
        Next::Label(to, _) => todo.push(*to),
        Next::Unwind(_, to) | Next::Case(_, to) => successor(to, todo),
    }
}
