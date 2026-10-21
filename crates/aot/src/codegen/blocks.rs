//! The blocks of a body reachable from its entry, with `linktime if`s decided.

use super::linktime::Linktime;
use crate::R;
use nir::{Body, Inst, Next};
use std::collections::{HashMap, HashSet};
use vm::World;

/// The labels of `body` that control can reach from its first block.
pub fn reachable(world: &mut World, linktime: &mut Linktime, body: &Body) -> R<HashSet<u64>> {
    let mut succ: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut first = None;
    let mut current = 0;
    for inst in &body.insts {
        let next = succ.entry(current).or_default();
        match inst {
            Inst::Label(l, _) => {
                current = l.0;
                first.get_or_insert(l.0);
            }
            Inst::LinktimeIf(cond, then, els) => {
                if let Next::Label(l, _) = if linktime.decide(world, cond)? { then } else { els } {
                    next.push(l.0);
                }
            }
            inst => inst.for_each_successor(&mut |l| next.push(l.0)),
        }
    }
    let mut seen = HashSet::new();
    let mut work: Vec<u64> = first.into_iter().collect();
    while let Some(l) = work.pop() {
        if seen.insert(l) {
            work.extend(succ.get(&l).into_iter().flatten().copied());
        }
    }
    Ok(seen)
}
