//! Which locals the lowering may keep out of memory, judged from all their uses.

use nir::{Body, Inst, Op, Pool, StrId, Val, ValId};
use rustc_hash::{FxHashMap, FxHashSet};

fn local(pool: &Pool, v: ValId) -> Option<u64> {
    match pool.val(v) {
        Val::Local(l, _) => Some(l.0),
        _ => None,
    }
}

/// The locals holding a `method` result that is only ever called with the same receiver,
/// by that receiver and the method's signature.
pub fn fusable_methods(body: &Body, pool: &Pool) -> FxHashMap<u64, (ValId, StrId)> {
    let mut methods: FxHashMap<u64, (ValId, StrId)> = body
        .insts
        .iter()
        .filter_map(|inst| match inst {
            Inst::Let { id, op: Op::Method { obj, sig }, .. } => Some((id.0, (*obj, *sig))),
            _ => None,
        })
        .collect();
    let mut rejected = FxHashSet::default();
    for inst in &body.insts {
        let operands = inst.operands();
        let skip = match inst {
            Inst::Let { op: Op::Call { ptr, args, .. }, .. } => match local(pool, *ptr) {
                Some(m) if methods.get(&m).is_some_and(|(obj, _)| args.first() == Some(obj)) => 1,
                _ => 0,
            },
            _ => 0,
        };
        rejected.extend(operands.into_iter().skip(skip).filter_map(|v| local(pool, v)));
    }
    methods.retain(|l, _| !rejected.contains(l));
    methods
}

/// The `var` cells used only as the cell of `varload` and `varstore`, which can live in a slot.
pub fn slot_vars(body: &Body, pool: &Pool) -> FxHashSet<u64> {
    let mut vars: FxHashSet<u64> = body
        .insts
        .iter()
        .filter_map(|inst| match inst {
            Inst::Let { id, op: Op::Var(_), .. } => Some(id.0),
            _ => None,
        })
        .collect();
    let mut rejected = FxHashSet::default();
    for inst in &body.insts {
        let operands = inst.operands();
        let skip = match inst {
            Inst::Let { op: Op::Varload(_) | Op::Varstore { .. }, .. } => 1,
            _ => 0,
        };
        rejected.extend(operands.into_iter().skip(skip).filter_map(|v| local(pool, v)));
    }
    vars.retain(|v| !rejected.contains(v));
    vars
}
