//! Folds a method's link-time conditions into jumps and drops the blocks no longer reached.

use crate::Vm;
use nir::{Body, Cond, Defn, DefnKind, Inst};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

impl Vm {
    /// `defn` with its link-time conditions folded, `defn` itself if it has none, or `None` if
    /// one reads a property only an accessor method provides.
    pub fn fold_linktime(&mut self, defn: Rc<Defn>) -> Option<Rc<Defn>> {
        let DefnKind::Define { ty, body } = &defn.kind else { return Some(defn) };
        if !body.insts.iter().any(|i| matches!(i, Inst::LinktimeIf(..))) {
            return Some(defn);
        }
        let mut insts = Vec::with_capacity(body.insts.len());
        for inst in &body.insts {
            insts.push(match inst {
                Inst::LinktimeIf(cond, then, other) => {
                    if !self.plain(cond) {
                        return None;
                    }
                    let taken = self.linktime_cond(cond, defn.name).ok()?;
                    Inst::Jump(if taken { then } else { other }.clone())
                }
                inst => inst.clone(),
            });
        }
        let body = reachable(Body { insts, ..body.clone() });
        Some(Rc::new(Defn { kind: DefnKind::Define { ty: *ty, body }, ..(*defn).clone() }))
    }

    /// Whether `cond` reads only properties the VM knows, with no accessor to call.
    fn plain(&mut self, cond: &Cond) -> bool {
        match cond {
            Cond::Complex { left, right, .. } => self.plain(left) && self.plain(right),
            Cond::Simple { property, .. } => {
                let name = self.world.pool.str(*property).to_owned();
                self.property_value(&name).is_ok()
            }
        }
    }
}

/// `body` without the blocks no branch, jump or handler reaches from the entry block.
fn reachable(body: Body) -> Body {
    let starts: Vec<usize> =
        (0..body.insts.len()).filter(|&i| matches!(body.insts[i], Inst::Label(..))).collect();
    let blocks: Vec<(usize, usize)> = starts
        .iter()
        .enumerate()
        .map(|(b, &s)| (s, starts.get(b + 1).copied().unwrap_or(body.insts.len())))
        .collect();
    let index: FxHashMap<u64, usize> = starts
        .iter()
        .enumerate()
        .filter_map(|(b, &s)| match &body.insts[s] {
            Inst::Label(l, _) => Some((l.0, b)),
            _ => None,
        })
        .collect();
    let (mut seen, mut todo) = (FxHashSet::default(), vec![0]);
    while let Some(b) = todo.pop() {
        if seen.insert(b) {
            let (start, end) = blocks[b];
            for inst in &body.insts[start..end] {
                inst.for_each_successor(&mut |l| todo.extend(index.get(&l.0)));
            }
        }
    }
    let (insts, positions) = blocks
        .iter()
        .enumerate()
        .filter(|(b, _)| seen.contains(b))
        .flat_map(|(_, &(start, end))| start..end)
        .map(|at| (body.insts[at].clone(), body.positions[at]))
        .unzip();
    Body { insts, positions, ..body }
}
