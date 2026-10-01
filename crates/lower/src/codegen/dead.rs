//! Blocks the entry does not reach, removed (NIR's `ControlFlow.removeDeadBlocks`).

use super::buf::Buf;
use nir::{Inst, Local, Next, PosId};
use rustc_hash::FxHashMap;

/// The blocks reached so far: the position of each label's first reference, blocks to visit.
struct Reached {
    pos: FxHashMap<Local, PosId>,
    todo: Vec<Local>,
}

impl Reached {
    /// Records the block `next` leads to, referenced at `pos`.
    fn successor(&mut self, next: &Next, pos: PosId) {
        match next {
            Next::None => {}
            Next::Label(to, _) => {
                if !self.pos.contains_key(to) {
                    self.pos.insert(*to, pos);
                    self.todo.push(*to);
                }
            }
            Next::Unwind(_, to) | Next::Case(_, to) => self.successor(to, pos),
        }
    }
}

/// The blocks of `buf` reachable from its first, in their order, each label at the position
/// of its first reference.
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
    let mut r = Reached { pos: FxHashMap::default(), todo: Vec::new() };
    r.successor(&Next::Label(*entry, Vec::new()), buf.positions[0]);
    while let Some(block) = r.todo.pop() {
        let start = starts[&block];
        for i in start + 1..=end(start) {
            let pos = buf.positions[i];
            match &buf.insts[i] {
                Inst::Let { unwind, .. } => r.successor(unwind, pos),
                Inst::Jump(n) | Inst::Throw(_, n) | Inst::Unreachable(n) => r.successor(n, pos),
                Inst::If(_, a, b) | Inst::LinktimeIf(_, a, b) => {
                    r.successor(a, pos);
                    r.successor(b, pos);
                }
                Inst::Switch(_, default, cases) => {
                    r.successor(default, pos);
                    cases.iter().for_each(|c| r.successor(c, pos));
                }
                Inst::Ret(_) | Inst::Label(..) => {}
            }
        }
    }
    let mut kept: Vec<(usize, PosId)> =
        starts.iter().filter_map(|(b, &s)| Some((s, *r.pos.get(b)?))).collect();
    kept.sort_unstable_by_key(|&(s, _)| s);
    let mut out = Buf::default();
    for (start, label_pos) in kept {
        out.insts.push(buf.insts[start].clone());
        out.positions.push(label_pos);
        for i in start + 1..=end(start) {
            out.insts.push(buf.insts[i].clone());
            out.positions.push(buf.positions[i]);
        }
    }
    out
}
