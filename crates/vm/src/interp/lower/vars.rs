//! Loads of `var`s that can read the var's slot instead of a copy of it, and values that can be
//! written to it directly.

use super::uses::{local, locals};
use nir::{Body, Inst, Op, Pool, ValId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Loads of slot `vars` whose every use follows in the same block before the next store to the
/// var, with the var's cell. Loads that an alias stands for (`aliased`) are left out: the
/// alias's uses are not counted here.
pub fn direct_var_reads(
    body: &Body,
    pool: &Pool,
    uses: &FxHashMap<u64, usize>,
    vars: &FxHashSet<u64>,
    aliased: &FxHashSet<u64>,
) -> FxHashMap<u64, ValId> {
    let mut direct = FxHashMap::default();
    // Loads of the current block not yet invalidated: load → (cell local, cell, uses seen).
    let mut open: FxHashMap<u64, (u64, ValId, usize)> = FxHashMap::default();
    for inst in &body.insts {
        if let Inst::Label(..) = inst {
            open.clear();
            continue;
        }
        if !open.is_empty() {
            inst.for_each_operand(&mut |v| {
                locals(pool, v, &mut |l| {
                    if let Some(entry) = open.get_mut(&l) {
                        entry.2 += 1;
                    }
                })
            });
            open.retain(|l, (_, cell, seen)| {
                let done = uses.get(l) == Some(seen);
                if done {
                    direct.insert(*l, *cell);
                }
                !done
            });
        }
        match inst {
            Inst::Let { id, op: Op::Varload(cell), .. } if !aliased.contains(&id.0) => {
                if let Some(c) = local(pool, *cell).filter(|c| vars.contains(c)) {
                    open.insert(id.0, (c, *cell, 0));
                }
            }
            Inst::Let { op: Op::Varstore { slot, .. }, .. } => {
                let stored = local(pool, *slot);
                open.retain(|_, (c, _, _)| Some(*c) != stored);
            }
            _ => {}
        }
    }
    direct
}

/// Values stored into a slot var by the instruction right after their definition and used
/// nowhere else, with the var's local: the value's instruction, or the moves into the block
/// parameter, write the var's slot. Values without a slot of their own (copies, var operations,
/// `known` constants, `aliases`) are left out.
pub fn direct_var_writes(
    body: &Body,
    pool: &Pool,
    uses: &FxHashMap<u64, usize>,
    vars: &FxHashSet<u64>,
    known: &FxHashMap<u64, u64>,
    aliases: &FxHashMap<u64, ValId>,
) -> FxHashMap<u64, u64> {
    let mut writes = FxHashMap::default();
    for pair in body.insts.windows(2) {
        let [defining, Inst::Let { op: Op::Varstore { slot, value }, .. }] = pair else { continue };
        let Some(v) = local(pool, *value).filter(|v| uses.get(v) == Some(&1) && !aliases.contains_key(v))
        else {
            continue;
        };
        let defined = match defining {
            Inst::Let { id, op, .. } => {
                id.0 == v
                    && !known.contains_key(&v)
                    && !matches!(op, Op::Copy(_) | Op::Varload(_) | Op::Varstore { .. } | Op::Var(_))
            }
            Inst::Label(_, params) => params.iter().any(|p| p.id.0 == v),
            _ => false,
        };
        if let Some(cell) = local(pool, *slot).filter(|c| defined && vars.contains(c)) {
            writes.insert(v, cell);
        }
    }
    writes
}
