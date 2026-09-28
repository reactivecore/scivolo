//! Folds a method's link-time conditions into jumps and drops the blocks no longer reached.

use crate::Vm;
use nir::{Body, Inst, Next};
use rustc_hash::{FxHashMap, FxHashSet};

impl Vm {
    /// `body` of method `g` without link-time conditions, or `None` if one cannot be decided.
    pub fn fold_linktime(&mut self, g: nir::GlobalId, body: &Body) -> Option<Body> {
        let mut insts = Vec::with_capacity(body.insts.len());
        for inst in &body.insts {
            insts.push(match inst {
                Inst::LinktimeIf(cond, then, other) => match self.linktime_cond(cond, g).ok()? {
                    true => Inst::Jump(then.clone()),
                    false => Inst::Jump(other.clone()),
                },
                other => other.clone(),
            });
        }
        Some(reachable(Body { insts, ..body.clone() }))
    }
}

/// `body` without the blocks no branch, jump or handler reaches from the entry block.
fn reachable(body: Body) -> Body {
    let starts: Vec<usize> =
        (0..body.insts.len()).filter(|&i| matches!(body.insts[i], Inst::Label(..))).collect();
    let block: FxHashMap<u64, usize> = starts
        .iter()
        .enumerate()
        .filter_map(|(b, &at)| match &body.insts[at] {
            Inst::Label(l, _) => Some((l.0, b)),
            _ => None,
        })
        .collect();
    let end = |b: usize| starts.get(b + 1).copied().unwrap_or(body.insts.len());
    let (mut seen, mut todo) = (FxHashSet::default(), vec![0]);
    while let Some(b) = todo.pop() {
        if !seen.insert(b) {
            continue;
        }
        for inst in &body.insts[starts[b]..end(b)] {
            for_each_target(inst, &mut |l| todo.extend(block.get(&l)));
        }
    }
    let keep = |at: usize| seen.contains(&(starts.partition_point(|&s| s <= at) - 1));
    let (insts, positions) = (0..body.insts.len())
        .filter(|&at| keep(at))
        .map(|at| (body.insts[at].clone(), body.positions[at]))
        .unzip();
    Body { insts, positions, ..body }
}

fn for_each_target(inst: &Inst, f: &mut impl FnMut(u64)) {
    let mut next = |n: &Next| {
        let mut n = n;
        loop {
            match n {
                Next::Label(l, _) => return f(l.0),
                Next::Unwind(_, to) | Next::Case(_, to) => n = to,
                Next::None => return,
            }
        }
    };
    match inst {
        Inst::Let { unwind, .. }
        | Inst::Throw(_, unwind)
        | Inst::Unreachable(unwind)
        | Inst::Jump(unwind) => next(unwind),
        Inst::If(_, t, e) | Inst::LinktimeIf(_, t, e) => {
            next(t);
            next(e);
        }
        Inst::Switch(_, d, cases) => {
            next(d);
            cases.iter().for_each(&mut next);
        }
        Inst::Label(..) | Inst::Ret(_) => {}
    }
}
