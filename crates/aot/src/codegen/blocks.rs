//! The blocks of a body reachable from its entry, with `linktime if`s decided.

use crate::R;
use nir::{Body, Inst, Next, Pool};
use std::collections::{HashMap, HashSet};

fn targets(next: &Next, out: &mut Vec<u64>) {
    match next {
        Next::Label(l, _) => out.push(l.0),
        Next::Unwind(_, to) | Next::Case(_, to) => targets(to, out),
        _ => {}
    }
}

/// The labels of `body` that control can reach from its first block.
pub fn reachable(pool: &Pool, body: &Body) -> R<HashSet<u64>> {
    let mut succ: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut first = None;
    let mut current = 0;
    for inst in &body.insts {
        let out = |l: u64, succ: &mut HashMap<u64, Vec<u64>>| succ.entry(l).or_default().len();
        let mut next = Vec::new();
        match inst {
            Inst::Label(l, _) => {
                current = l.0;
                first.get_or_insert(l.0);
                out(current, &mut succ);
            }
            Inst::Let { unwind, .. } | Inst::Throw(_, unwind) => targets(unwind, &mut next),
            Inst::Jump(to) => targets(to, &mut next),
            Inst::If(_, then, els) => [then, els].into_iter().for_each(|n| targets(n, &mut next)),
            Inst::Switch(_, default, cases) => {
                targets(default, &mut next);
                cases.iter().for_each(|c| targets(c, &mut next));
            }
            Inst::LinktimeIf(cond, then, els) => {
                let taken = super::linktime::decide(pool, cond)?;
                targets(if taken { then } else { els }, &mut next);
            }
            Inst::Ret(_) | Inst::Unreachable(_) => {}
        }
        succ.entry(current).or_default().extend(next);
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
